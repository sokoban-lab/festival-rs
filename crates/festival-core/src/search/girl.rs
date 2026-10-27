// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! GIRL value function and epsilon-greedy tree descent (`girl.cpp` /
//! `greedy.cpp`).
//!
//! The trained weights are kept verbatim: they are part of the algorithm, not
//! tuning knobs. Features are computed in the same order as the C++ code and
//! evaluated in `f64`, exactly like `score_to_value`; only the back-propagated
//! per-node value is stored in the `f32` score field.

use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::level::Level;
use crate::planning::{overlap, park_order};
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::search::tree::{ExpansionId, NONE_EXPANSION, Tree};
use crate::solver::LevelContext;

/// Hard-coded push weights (8 features), `girl.cpp`.
pub const PUSH_WEIGHTS: [f64; 8] = [
    0.468630, -0.177816, -0.233347, 0.052064, 0.326019, 0.164538, 0.348762, -0.141263,
];

/// Hard-coded pull weights (6 features), `girl.cpp`.
pub const PULL_WEIGHTS: [f64; 6] = [-0.185353, -0.217472, 0.154112, 0.382115, 0.471096, 0.195508];

const GAMMA: f64 = 0.95;
const NO_VALUE: f32 = -1_000_000.0;
const MAX_VALUE: f64 = -1_000_000.0;

/// Per-level GIRL parameters (`init_girl_variables`).
#[derive(Clone, Debug)]
pub struct GirlParams {
    /// Pull distance from every cell to the set of targets.
    pub distance: [u32; MAX_INNER],
    pub max_push_dist: u32,
    pub gamma_table: [f64; MAX_BOXES + 1],
    pub inv_n_boxes: f64,
    pub dist_normalizer: f64,
    pub inv_epsilon: u32,
    pub boxes: usize,
}

impl GirlParams {
    pub fn compute(level: &Level) -> Self {
        let mut distance = [INF; MAX_INNER];
        crate::distance::get_distance_from_targets(level, &level.board, &mut distance);

        let mut max_push_dist = 0u32;
        for &value in distance.iter() {
            if value != INF && value > max_push_dist {
                max_push_dist = value;
            }
        }

        let mut gamma_table = [0.0f64; MAX_BOXES + 1];
        let boxes = level.box_count;
        for (i, slot) in gamma_table.iter_mut().enumerate().take(boxes + 1) {
            *slot = GAMMA.powi(i as i32);
        }

        let inv_n_boxes = 1.0 / boxes as f64;
        let dist_normalizer = 1.0 / ((boxes as u64 * max_push_dist as u64) as f64);

        Self {
            distance,
            max_push_dist,
            gamma_table,
            inv_n_boxes,
            dist_normalizer,
            inv_epsilon: (boxes + 1) as u32,
            boxes,
        }
    }
}

/// Sum of the per-box pull distance to the nearest target (`get_girl_distance`).
pub fn get_girl_distance(board: &crate::board::Board, girl: &GirlParams) -> i32 {
    let mut sum = 0i32;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            sum += girl.distance[crate::board::Board::flat_index(y, x)] as i32;
        }
    }
    sum
}

/// `prepare_vector_of_basic_features` + `score_to_value`.
pub fn score_to_value(score: &ScoreElement, pull_mode: bool, girl: &GirlParams) -> f64 {
    let n = girl.boxes;
    let packed = score.packed_boxes as f64;
    let tgt = score.boxes_on_targets as f64;
    let connectivity = score.connectivity as f64;
    let dist = score.dist_to_targets as f64;
    let overlap = score.overlap as f64;

    if pull_mode {
        let features = [
            tgt * girl.inv_n_boxes,
            connectivity * girl.inv_n_boxes,
            dist * girl.dist_normalizer,
            girl.gamma_table[score.boxes_on_targets as usize],
            girl.gamma_table[n],
            1.0,
        ];
        features
            .iter()
            .zip(PULL_WEIGHTS.iter())
            .map(|(feature, weight)| feature * weight)
            .sum()
    } else {
        let features = [
            packed * girl.inv_n_boxes,
            tgt * girl.inv_n_boxes,
            connectivity * girl.inv_n_boxes,
            dist * girl.dist_normalizer,
            overlap * girl.inv_n_boxes,
            girl.gamma_table[n - score.boxes_on_targets as usize],
            girl.gamma_table[n],
            1.0,
        ];
        features
            .iter()
            .zip(PUSH_WEIGHTS.iter())
            .map(|(feature, weight)| feature * weight)
            .sum()
    }
}

/// `fill_girl_scores`.
pub fn fill_girl_scores(
    ctx: &LevelContext,
    helper: &Helper,
    board: &crate::board::Board,
    score: &mut ScoreElement,
    pull_mode: bool,
) {
    score.dist_to_targets = get_girl_distance(board, &ctx.girl);
    if !pull_mode {
        score.packed_boxes = park_order::score_parked_boxes(ctx, board, helper) as i16;
        score.overlap = overlap::overlap_score(ctx, board, helper) as i16;
    }
}

impl Tree {
    /// `set_value_to_position_tree`.
    fn set_value_to_position(&mut self, expansion: ExpansionId, girl: &GirlParams) {
        let node = self.expansions[expansion as usize].node;
        let value = score_to_value(&self.nodes[node as usize].score, self.pull_mode, girl);
        self.nodes[node as usize].score.value = value as f32;
    }

    /// `set_values_to_moves_tree`.
    pub fn set_values_to_moves(&mut self, expansion: ExpansionId, girl: &GirlParams) -> Result<()> {
        self.set_value_to_position(expansion, girl);

        let (start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.move_hash_place, e.moves_num as usize)
        };
        for i in 0..count {
            if self.move_hashes[start + i].deadlocked {
                continue;
            }
            let hash = self.move_hashes[start + i].hash;
            let place = self
                .find_node_by_hash(hash)
                .ok_or_else(|| SolveError::Internal("missing node".into()))?;
            let value = score_to_value(&self.nodes[place as usize].score, self.pull_mode, girl);
            self.nodes[place as usize].score.value = value as f32;
        }
        Ok(())
    }

    /// `handle_new_pos_in_girl_mode_tree`.
    pub fn handle_new_pos_in_girl_mode(
        &mut self,
        ctx: &LevelContext,
        expansion: ExpansionId,
    ) -> Result<()> {
        self.set_values_to_moves(expansion, &ctx.girl)?;
        self.back_propagate_values(ctx, expansion)
    }

    /// `recompute_values_tree`: the node value is the best live son value.
    fn recompute_values(&mut self, expansion: ExpansionId) -> Result<()> {
        let node = self.expansions[expansion as usize].node;
        if self.nodes[node as usize].deadlocked {
            return Err(SolveError::Internal(
                "recomputing a deadlocked position".into(),
            ));
        }

        let (start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.move_hash_place, e.moves_num as usize)
        };

        let mut value = NO_VALUE;
        for i in 0..count {
            if self.move_hashes[start + i].deadlocked {
                continue;
            }
            let hash = self.move_hashes[start + i].hash;
            let place = self
                .find_node_by_hash(hash)
                .ok_or_else(|| SolveError::Internal("missing node".into()))?;
            let son_value = self.nodes[place as usize].score.value;
            if son_value > value {
                value = son_value;
            }
        }

        if value == NO_VALUE {
            return Err(SolveError::Internal("score problem".into()));
        }

        self.nodes[node as usize].score.value = value;
        Ok(())
    }

    /// `choose_cyclic_son_tree`: round-robin over live sons.
    fn choose_cyclic_son(&mut self, expansion: ExpansionId) -> Result<usize> {
        let node = self.expansions[expansion as usize].node;
        if self.nodes[node as usize].deadlocked {
            return Err(SolveError::Internal(
                "choosing son for deadlocked position".into(),
            ));
        }

        let (start, count, mut son) = {
            let e = &self.expansions[expansion as usize];
            (
                e.move_hash_place,
                e.moves_num as usize,
                e.next_son_to_check as usize,
            )
        };
        if count == 0 {
            return Err(SolveError::Internal("no sons".into()));
        }

        let first = son;
        while self.move_hashes[start + son].deadlocked {
            son = (son + 1) % count;
            if son == first {
                return Err(SolveError::Internal("returned to first".into()));
            }
        }

        self.expansions[expansion as usize].next_son_to_check = ((son + 1) % count) as u16;
        Ok(son)
    }

    /// `choose_best_value_son_tree`; the first best son wins ties.
    fn choose_best_value_son(&self, expansion: ExpansionId) -> Result<usize> {
        let node = self.expansions[expansion as usize].node;
        if self.nodes[node as usize].deadlocked {
            return Err(SolveError::Internal(
                "choosing son for deadlocked position".into(),
            ));
        }

        let (start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.move_hash_place, e.moves_num as usize)
        };

        let mut best_son: Option<usize> = None;
        let mut best_value = MAX_VALUE;
        for i in 0..count {
            if self.move_hashes[start + i].deadlocked {
                continue;
            }
            let hash = self.move_hashes[start + i].hash;
            let place = self
                .find_node_by_hash(hash)
                .ok_or_else(|| SolveError::Internal("missing node".into()))?;
            let value = self.nodes[place as usize].score.value as f64;
            if best_son.is_none() || value > best_value {
                best_value = value;
                best_son = Some(i);
            }
        }

        best_son.ok_or_else(|| SolveError::Internal("no best son".into()))
    }

    /// `pick_son_Epsilon_greedy_tree`.
    fn pick_son_epsilon_greedy(
        &mut self,
        expansion: ExpansionId,
        inv_epsilon: u32,
    ) -> Result<usize> {
        let node = self.expansions[expansion as usize].node;
        if self.nodes[node as usize].deadlocked {
            return Err(SolveError::Internal("epsilon deadlocked".into()));
        }

        if inv_epsilon != 0 && self.expansions[expansion as usize].subtree_size % inv_epsilon == 0 {
            return self.choose_cyclic_son(expansion);
        }
        self.choose_best_value_son(expansion)
    }

    /// `go_to_root_greedy_tree`: the unprocessed root with the smallest subtree.
    fn go_to_root_greedy(&self) -> Option<ExpansionId> {
        let mut best: Option<ExpansionId> = None;
        let mut min_subtree_size = u32::MAX;

        for (index, expansion) in self.expansions.iter().enumerate() {
            if self.nodes[expansion.node as usize].deadlocked {
                continue;
            }
            if expansion.depth > 0 {
                break; // all roots were processed
            }
            if best.is_none() || expansion.subtree_size < min_subtree_size {
                min_subtree_size = expansion.subtree_size;
                best = Some(index as ExpansionId);
            }
        }

        best
    }

    /// `choose_pos_son_to_expand_Epsilon_greedy_tree`. Returns the expansion
    /// to expand and the chosen son, or `None` when every root subtree is done.
    pub fn choose_pos_son_to_expand_epsilon_greedy(
        &mut self,
        ctx: &LevelContext,
    ) -> Result<Option<(ExpansionId, usize)>> {
        let Some(mut expansion) = self.go_to_root_greedy() else {
            return Ok(None);
        };

        loop {
            self.set_deadlock_status(expansion, ctx);

            let node = self.expansions[expansion as usize].node;
            if self.nodes[node as usize].deadlocked {
                expansion = match self.go_to_root_greedy() {
                    Some(next) => next,
                    None => return Ok(None),
                };
                continue;
            }

            self.recompute_values(expansion)?;
            let son = self.pick_son_epsilon_greedy(expansion, ctx.girl.inv_epsilon)?;

            let hash = {
                let e = &self.expansions[expansion as usize];
                self.move_hashes[e.move_hash_place + son].hash
            };
            let place = self
                .find_node_by_hash(hash)
                .ok_or_else(|| SolveError::Internal("missing son".into()))?;

            let next = self.nodes[place as usize].expansion;
            if next != NONE_EXPANSION {
                expansion = next;
            } else {
                return Ok(Some((expansion, son)));
            }
        }
    }

    /// `back_propagate_values_tree`: propagate the best son value to ancestors.
    pub fn back_propagate_values(
        &mut self,
        ctx: &LevelContext,
        expansion: ExpansionId,
    ) -> Result<()> {
        let mut current = Some(expansion);

        while let Some(id) = current {
            let node = self.expansions[id as usize].node;
            if self.nodes[node as usize].deadlocked {
                break;
            }

            let (start, count, father) = {
                let e = &self.expansions[id as usize];
                (e.move_hash_place, e.moves_num as usize, e.father)
            };

            let mut max_value = NO_VALUE;
            let mut has_son = false;
            for i in 0..count {
                if self.move_hashes[start + i].deadlocked {
                    continue;
                }
                let hash = self.move_hashes[start + i].hash;
                let place = self
                    .find_node_by_hash(hash)
                    .ok_or_else(|| SolveError::Internal("missing node".into()))?;
                let value = self.nodes[place as usize].score.value;
                if value > max_value {
                    max_value = value;
                }
                has_son = true;
            }

            if !has_son {
                if !self.node_is_solved(id, ctx) {
                    return Err(SolveError::Internal("no sons?".into()));
                }
                max_value = self.nodes[node as usize].score.value;
            }

            self.nodes[node as usize].score.value = max_value;
            if max_value == NO_VALUE {
                return Err(SolveError::Internal("max val".into()));
            }

            current = if father == NONE_EXPANSION {
                None
            } else {
                Some(father)
            };
        }

        Ok(())
    }

    /// `find_best_RL_parking_node`: best pull-mode value that is close to the
    /// maximum along its root path.
    pub fn find_best_rl_parking_node(&self, girl: &GirlParams) -> Option<ExpansionId> {
        let max_allowed_depth = MAX_SOL_LEN.saturating_sub(girl.boxes);

        let mut best: Option<ExpansionId> = None;
        let mut max_value = MAX_VALUE;

        for (index, expansion) in self.expansions.iter().enumerate() {
            if expansion.depth as usize >= max_allowed_depth {
                continue;
            }
            if self.nodes[expansion.node as usize].deadlocked {
                continue;
            }
            let value = score_to_value(&self.nodes[expansion.node as usize].score, true, girl);
            if value > max_value {
                max_value = value;
                best = Some(index as ExpansionId);
            }
        }

        let mut best = best?;
        let mut current = Some(best);
        while let Some(id) = current {
            let node = self.expansions[id as usize].node;
            let value = score_to_value(&self.nodes[node as usize].score, true, girl);
            if value > max_value - 0.1 {
                best = id;
            }
            let father = self.expansions[id as usize].father;
            current = if father == NONE_EXPANSION {
                None
            } else {
                Some(father)
            };
        }

        Some(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn gamma_and_normalizers_follow_boxes() {
        let girl = GirlParams::compute(&level("#######\n#@ $ .#\n#######\n"));
        assert_eq!(girl.boxes, 1);
        assert!((girl.gamma_table[0] - 1.0).abs() < f64::EPSILON);
        assert!((girl.gamma_table[1] - 0.95).abs() < 1e-15);
        assert_eq!(girl.inv_epsilon, 2);
        assert_eq!(girl.max_push_dist, 3);
        assert!((girl.dist_normalizer - 1.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn value_matches_hand_computed_push_features() {
        let girl = GirlParams::compute(&level("#######\n#@ $ .#\n#######\n"));
        let score = ScoreElement {
            packed_boxes: 0,
            boxes_on_targets: 0,
            connectivity: 3,
            dist_to_targets: 2,
            overlap: 1,
            ..ScoreElement::default()
        };
        // features: 0, 0, 3, 2*dist_normalizer, 1, gamma[1], gamma[1], 1
        let expected = 3.0 * PUSH_WEIGHTS[2]
            + 2.0 * girl.dist_normalizer * PUSH_WEIGHTS[3]
            + 1.0 * PUSH_WEIGHTS[4]
            + girl.gamma_table[1] * PUSH_WEIGHTS[5]
            + girl.gamma_table[1] * PUSH_WEIGHTS[6]
            + PUSH_WEIGHTS[7];
        assert!((score_to_value(&score, false, &girl) - expected).abs() < 1e-12);
    }

    #[test]
    fn value_matches_hand_computed_pull_features() {
        let girl = GirlParams::compute(&level("#######\n#@ $ .#\n#######\n"));
        let score = ScoreElement {
            boxes_on_targets: 1,
            connectivity: 2,
            dist_to_targets: 2,
            ..ScoreElement::default()
        };
        let expected = 1.0 * PULL_WEIGHTS[0]
            + 2.0 * PULL_WEIGHTS[1]
            + 2.0 * girl.dist_normalizer * PULL_WEIGHTS[2]
            + girl.gamma_table[1] * PULL_WEIGHTS[3]
            + girl.gamma_table[1] * PULL_WEIGHTS[4]
            + PULL_WEIGHTS[5];
        assert!((score_to_value(&score, true, &girl) - expected).abs() < 1e-12);
    }
}
