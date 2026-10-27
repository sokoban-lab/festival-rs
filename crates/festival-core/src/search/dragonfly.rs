// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Dragonfly pull search (`dragonfly.cpp` / `dragonfly_queue.cpp`).
//!
//! A greedy best-first walk over pull moves, ordered by the Hungarian
//! distance from the initial box positions (bases), then by the number of
//! packed boxes. The node pool is per-search; board states are rebuilt from
//! the root plus the move chain, matching the C++ `0xff` sentinel scheme.

use std::time::{Duration, Instant};

use crate::analysis::match_distance::compute_base_distance;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::corral::detect_corral;
use crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls;
use crate::deadlock::oracle::move_is_deadlocked;
use crate::deadlock::state::DeadlockState;
use crate::error::{Result, SolveError};
use crate::moves::{Move, apply_move, find_possible_moves};
use crate::search::helper::Helper;
use crate::search::packing::{compute_start_zones, put_sokoban_in_zone};
use crate::search::perimeter::Perimeter;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

const NO_ROOT: u8 = 0xff;
const MAX_DRAGONFLY_ROOTS: usize = 200;

#[derive(Clone, Copy, Debug, Default)]
pub struct DragonflyNode {
    pub board: u8,
    pub depth: u16,
    pub father: i32,
    pub move_from: u16,
    pub move_to: u16,
    pub player_position: u8,
    pub dist: i32,
    pub packed: u16,
}

#[derive(Clone, Debug)]
pub struct DragonflyConfig {
    pub time_limit: Option<Duration>,
    pub max_nodes: usize,
    /// Collect every created node (diagnostic/differential use).
    pub trace: bool,
}

impl Default for DragonflyConfig {
    fn default() -> Self {
        Self {
            time_limit: None,
            max_nodes: 1 << 22,
            trace: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DragonflyTraceNode {
    pub hash: u64,
    pub depth: u16,
    pub father: i32,
    pub dist: i32,
    pub packed: u16,
}

#[derive(Clone, Debug, Default)]
pub struct DragonflyRun {
    pub solved: bool,
    pub nodes: usize,
    pub roots: usize,
    pub expansions: u64,
    pub elapsed: Duration,
    pub trace: Option<Vec<DragonflyTraceNode>>,
}

/// `is_better_dragonfly_score`: shorter base distance, then more packed boxes.
fn is_better_score(a: &DragonflyNode, b: &DragonflyNode) -> bool {
    if a.dist < b.dist {
        return true;
    }
    if a.dist > b.dist {
        return false;
    }
    a.packed > b.packed
}

fn is_better_element(nodes: &[DragonflyNode], first: i32, second: i32) -> bool {
    let a = &nodes[first as usize];
    let b = &nodes[second as usize];
    if is_better_score(a, b) {
        return true;
    }
    if is_better_score(b, a) {
        return false;
    }
    first < second
}

#[derive(Debug, Default)]
struct DragonflyQueue {
    elements: Vec<i32>,
}

impl DragonflyQueue {
    fn clear(&mut self) {
        self.elements.clear();
    }

    fn sift_down(&mut self, nodes: &[DragonflyNode], mut i: usize) {
        loop {
            let left = i * 2 + 1;
            let right = i * 2 + 2;
            let mut largest = i;

            if left < self.elements.len()
                && is_better_element(nodes, self.elements[left], self.elements[i])
            {
                largest = left;
            }
            if right < self.elements.len()
                && is_better_element(nodes, self.elements[right], self.elements[largest])
            {
                largest = right;
            }
            if largest == i {
                return;
            }
            self.elements.swap(i, largest);
            i = largest;
        }
    }

    fn insert(&mut self, nodes: &[DragonflyNode], value: i32) {
        self.elements.push(value);
        let mut i = self.elements.len() - 1;
        while i > 0 {
            let parent = (i - 1) / 2;
            if !is_better_element(nodes, value, self.elements[parent]) {
                break;
            }
            self.elements[i] = self.elements[parent];
            i = parent;
        }
        self.elements[i] = value;
    }

    fn extract_max(&mut self, nodes: &[DragonflyNode]) -> Option<i32> {
        if self.elements.is_empty() {
            return None;
        }
        let result = self.elements[0];
        let last = self.elements.pop().expect("non-empty");
        if let Some(slot) = self.elements.first_mut() {
            *slot = last;
            self.sift_down(nodes, 0);
        }
        Some(result)
    }
}

struct Dragonfly {
    nodes: Vec<DragonflyNode>,
    roots: Vec<Board>,
    queue: DragonflyQueue,
}

impl Dragonfly {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: Vec::new(),
            queue: DragonflyQueue::default(),
        }
    }

    fn dist_from_bases(ctx: &LevelContext, board: &Board) -> i32 {
        let mut score = ScoreElement::default();
        compute_base_distance(&ctx.level, &ctx.distances, board, &mut score, &[], &mut []);
        score.dist_to_targets
    }

    fn add_root(
        &mut self,
        ctx: &LevelContext,
        board: &Board,
        perimeter: &mut Perimeter,
    ) -> Result<()> {
        if self.roots.len() >= MAX_DRAGONFLY_ROOTS {
            return Err(SolveError::ResourceExhausted);
        }

        self.roots.push(*board);
        let node = DragonflyNode {
            board: (self.roots.len() - 1) as u8,
            depth: 0,
            father: -1,
            move_from: u16::MAX,
            move_to: u16::MAX,
            player_position: 4,
            dist: Self::dist_from_bases(ctx, board),
            packed: board.boxes_on_bases(&ctx.level.initial) as u16,
        };
        self.nodes.push(node);
        let index = (self.nodes.len() - 1) as i32;
        self.queue.insert(&self.nodes, index);

        perimeter.update_dragonfly(&[board.hash()], &[0], 0);
        Ok(())
    }

    /// `dragonfly_get_board`: rebuild the board from the root and move chain.
    fn node_board(&self, ctx: &LevelContext, index: usize) -> Result<Board> {
        let mut node = self.nodes[index];
        let player = node.player_position;

        if node.board != NO_ROOT {
            return Ok(self.roots[node.board as usize]);
        }

        let mut moves: Vec<(u16, u16)> = Vec::new();
        while node.board == NO_ROOT {
            if node.father < 0 {
                return Err(SolveError::Internal("dragonfly: missing root".into()));
            }
            if moves.len() >= MAX_SOL_LEN {
                return Err(SolveError::TooManyMoves);
            }
            moves.push((node.move_from, node.move_to));
            node = self.nodes[node.father as usize];
        }

        let mut board = self.roots[node.board as usize];
        for &(from, to) in moves.iter().rev() {
            let (from_y, from_x) = ctx.level.coord(from as usize);
            if board.cell(from_y, from_x) & BOX == 0 {
                return Err(SolveError::Internal("dragonfly: missing box".into()));
            }
            board.and_not(from_y, from_x, BOX);

            let (to_y, to_x) = ctx.level.coord(to as usize);
            if board.cell(to_y, to_x) & OCCUPIED != 0 {
                return Err(SolveError::Internal("dragonfly: occupied target".into()));
            }
            board.or(to_y, to_x, BOX);
        }

        board.clear_sokoban_inplace();
        if player as usize >= 4 || moves.is_empty() {
            return Err(SolveError::Internal("dragonfly: bad player".into()));
        }
        let (to_y, to_x) = ctx.level.coord(moves[0].1 as usize);
        let sok_y = to_y as isize + DELTA_Y[player as usize];
        let sok_x = to_x as isize + DELTA_X[player as usize];
        if sok_y < 0
            || sok_x < 0
            || sok_y as usize >= board.height()
            || sok_x as usize >= board.width()
        {
            return Err(SolveError::Internal(
                "dragonfly: player outside board".into(),
            ));
        }
        board.or(sok_y as usize, sok_x as usize, SOKOBAN);
        board.expand_sokoban_cloud();

        Ok(board)
    }

    fn node_is_solved(&self, ctx: &LevelContext, index: usize) -> Result<bool> {
        if self.nodes[index].dist > 0 {
            return Ok(false);
        }
        let board = self.node_board(ctx, index)?;
        Ok(board.is_solved(true, &ctx.level.initial, ctx.level.initial_sokoban))
    }

    /// `mark_moves_when_corral`.
    fn mark_moves_when_corral(
        &self,
        ctx: &LevelContext,
        deadlock: &DeadlockState,
        board: &Board,
        moves: &[Move],
        last_move: Option<usize>,
    ) -> Vec<bool> {
        let mut possible = vec![true; moves.len()];

        if crate::bfs::get_connectivity(&ctx.level, board) == 1 {
            return possible;
        }

        let walls =
            turn_fixed_boxes_into_walls(&ctx.level, &ctx.distances, board, &deadlock.envelopes);
        let Some(options) = detect_corral(&ctx.level, &ctx.distances, &walls) else {
            return possible;
        };
        let _ = options;

        let Some(last_move) = last_move else {
            // A root has no incoming move; the C++ code indexes -1 here.
            return possible;
        };
        let (y, x) = ctx.level.coord(last_move);

        for (i, mv) in moves.iter().enumerate() {
            if mv.from as usize == last_move {
                possible[i] = false; // another macro move will deal with it
                continue;
            }

            let mut child = *board;
            apply_move(&mut child, mv, SearchMode::Normal, &ctx.level);

            if child.is_solved(true, &ctx.level.initial, ctx.level.initial_sokoban) {
                continue;
            }

            let mut reachable = false;
            for dir in 0..4 {
                let cy = y as isize + DELTA_Y[dir];
                let cx = x as isize + DELTA_X[dir];
                if (child.get(cy, cx) ^ board.get(cy, cx)) & SOKOBAN != 0 {
                    reachable = true;
                    break;
                }
            }
            if !reachable {
                possible[i] = false;
            }
        }

        possible
    }

    /// `dragonfly_expand_node`; returns whether the node was expanded.
    fn expand_node(
        &mut self,
        ctx: &LevelContext,
        deadlock: &mut DeadlockState,
        helper: &mut Helper,
        perimeter: &mut Perimeter,
        index: usize,
    ) -> Result<bool> {
        let e = self.nodes[index];

        if e.depth as usize >= MAX_SOL_LEN {
            return Ok(false);
        }

        if e.board == NO_ROOT {
            let board = self.node_board(ctx, e.father as usize)?;
            let mv = Move {
                from: e.move_from,
                to: e.move_to,
                sokoban_position: e.player_position,
                kill: false,
                base: false,
                pull: true,
                attr: crate::moves::MoveAttr::default(),
            };
            if move_is_deadlocked(ctx, deadlock, &board, &mv, true, SearchMode::Dragonfly)? {
                return Ok(false);
            }
        }

        let board = self.node_board(ctx, index)?;
        let (moves, _has_corral) =
            find_possible_moves(ctx, &board, true, SearchMode::Normal, helper, deadlock)?;
        if moves.is_empty() {
            return Ok(false);
        }

        let last_move = if e.move_to != u16::MAX {
            Some(e.move_to as usize)
        } else {
            None
        };
        let possible = self.mark_moves_when_corral(ctx, deadlock, &board, &moves, last_move);

        let mut hashes = Vec::with_capacity(moves.len());
        let mut packed = Vec::with_capacity(moves.len());
        for mv in &moves {
            let mut child = board;
            apply_move(&mut child, mv, SearchMode::Normal, &ctx.level);
            hashes.push(child.hash());
            packed.push(child.boxes_on_bases(&ctx.level.initial) as u16);
        }

        let mut visited = perimeter.mark_dragonfly_visited(&hashes);

        let mut base_score = ScoreElement::default();
        let mut scores = vec![ScoreElement::default(); moves.len()];
        compute_base_distance(
            &ctx.level,
            &ctx.distances,
            &board,
            &mut base_score,
            &moves,
            &mut scores,
        );

        if e.dist >= 0 && base_score.dist_to_targets != e.dist {
            return Err(SolveError::Internal("dragonfly: dist mismatch".into()));
        }

        for i in 0..moves.len() {
            if visited[i] == 1 {
                continue;
            }
            if !possible[i] {
                visited[i] = 1;
                continue;
            }
            if scores[i].dist_to_targets >= INF as i32 {
                visited[i] = 1;
                continue;
            }

            let mut node = DragonflyNode {
                board: NO_ROOT,
                depth: e.depth + 1,
                father: index as i32,
                move_from: moves[i].from,
                move_to: moves[i].to,
                player_position: moves[i].sokoban_position,
                dist: scores[i].dist_to_targets,
                packed: packed[i],
            };

            if visited[i] < 0 {
                if !helper.perimeter_found {
                    helper.perimeter_found = true;
                }
                node.dist = visited[i];
            }

            self.nodes.push(node);
            let new_index = (self.nodes.len() - 1) as i32;
            self.queue.insert(&self.nodes, new_index);
        }

        perimeter.update_dragonfly(&hashes, &visited, e.depth + 1);
        Ok(true)
    }
}

/// Port of `dragonfly_search`.
pub fn run_dragonfly(
    ctx: &LevelContext,
    deadlock: &mut DeadlockState,
    helper: &mut Helper,
    perimeter: &mut Perimeter,
    config: &DragonflyConfig,
    board_in: &Board,
) -> Result<DragonflyRun> {
    if config.time_limit == Some(Duration::ZERO) {
        return Ok(DragonflyRun::default());
    }

    let started = Instant::now();
    helper.perimeter_found = false;

    let mut root_board = *board_in;
    root_board.enter_reverse_mode(&ctx.level.initial, false);

    let (zones, zones_num) = compute_start_zones(ctx);
    let mut dragonfly = Dragonfly::new();

    for z in 0..zones_num as u32 {
        let mut root = root_board;
        put_sokoban_in_zone(&mut root, &zones, z);
        dragonfly.add_root(ctx, &root, perimeter)?;
    }

    let mut solved = false;
    let mut expansions = 0u64;
    let mut best_so_far: Option<usize> = if dragonfly.nodes.is_empty() {
        None
    } else {
        Some(0)
    };

    loop {
        if let Some(limit) = config.time_limit {
            if started.elapsed().as_secs() > limit.as_secs() {
                break;
            }
        }

        let Some(node_index) = dragonfly.queue.extract_max(&dragonfly.nodes) else {
            break;
        };
        let node_index = node_index as usize;

        if dragonfly.node_is_solved(ctx, node_index)? {
            solved = true;
            break;
        }

        let expanded = dragonfly.expand_node(ctx, deadlock, helper, perimeter, node_index)?;
        expansions += 1;

        let dist = dragonfly.nodes[node_index].dist;
        if expanded && dist >= 0 {
            let best_dist = best_so_far.map(|index| dragonfly.nodes[index].dist);
            if best_dist.is_none_or(|best| dist < best) {
                best_so_far = Some(node_index);
            }
        }

        if dragonfly.nodes.len() >= config.max_nodes.saturating_sub(MAX_MOVES) {
            break;
        }
        if perimeter.is_full() {
            break;
        }
    }

    dragonfly.queue.clear();

    let trace = if config.trace {
        let mut rows = Vec::with_capacity(dragonfly.nodes.len());
        for index in 0..dragonfly.nodes.len() {
            let board = dragonfly.node_board(ctx, index)?;
            let node = dragonfly.nodes[index];
            rows.push(DragonflyTraceNode {
                hash: board.hash(),
                depth: node.depth,
                father: node.father,
                dist: node.dist,
                packed: node.packed,
            });
        }
        Some(rows)
    } else {
        None
    };

    Ok(DragonflyRun {
        solved,
        nodes: dragonfly.nodes.len(),
        roots: dragonfly.roots.len(),
        expansions,
        elapsed: started.elapsed(),
        trace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;
    use crate::solver::SearchClock;

    fn context(text: &str) -> (LevelContext, DeadlockState) {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = SearchClock::new(Some(Duration::from_secs(60)), None);
        let ctx = LevelContext::analyse(level, &clock).unwrap();
        let deadlock = DeadlockState::prepare(&ctx).unwrap();
        (ctx, deadlock)
    }

    fn node(dist: i32, packed: u16) -> DragonflyNode {
        DragonflyNode {
            dist,
            packed,
            ..DragonflyNode::default()
        }
    }

    #[test]
    fn queue_prefers_distance_then_packed_then_index() {
        let nodes = vec![node(5, 0), node(3, 1), node(3, 2), node(3, 2)];
        assert!(!is_better_element(&nodes, 0, 1));
        assert!(is_better_element(&nodes, 1, 0));
        assert!(!is_better_element(&nodes, 1, 2));
        assert!(is_better_element(&nodes, 2, 3));
        assert!(!is_better_element(&nodes, 3, 2));
    }

    #[test]
    fn dragonfly_walks_a_small_level() {
        let (ctx, mut deadlock) = context("########\n#@ $ . #\n#      #\n# $ .  #\n########\n");
        let mut tree = crate::search::tree::Tree::new_with_level(12, &ctx.level).unwrap();
        tree.prepare_deadlock(&ctx).unwrap();
        let mut helper = Helper::default();
        let mut perimeter = Perimeter::new(12);
        let config = DragonflyConfig {
            time_limit: Some(Duration::from_secs(5)),
            max_nodes: 100_000,
            ..DragonflyConfig::default()
        };
        let run = run_dragonfly(
            &ctx,
            &mut deadlock,
            &mut helper,
            &mut perimeter,
            &config,
            &ctx.level.board,
        )
        .unwrap();
        assert!(run.nodes > run.roots);
        assert!(run.expansions > 0);
    }
}
