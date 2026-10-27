// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Arena for the auxiliary deadlock searches (`positions.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::state::DeadlockState;
use crate::deadlock::zone::work_left_in_deadlock_zone;
use crate::error::Result;
use crate::moves::{Move, apply_move, find_possible_moves};
use crate::search::helper::Helper;
use crate::search::score::{ScoreElement, score_board};
use crate::solver::LevelContext;

pub struct Position {
    pub moves: Vec<Move>,
    pub scores: Vec<ScoreElement>,
    pub hashes: Vec<u64>,
    /// Index of the position this move reaches, if already visited.
    pub place_in_positions: Vec<Option<usize>>,
    pub move_deadlocked: Vec<bool>,
    pub board: Board,
    pub father: Option<usize>,
    pub depth: usize,
    pub position_score: ScoreElement,
    pub position_hash: u64,
    pub position_deadlocked: bool,
    pub weight: i32,
    pub pull_mode: bool,
    pub has_corral: bool,
    pub hash_indicators: [u64; 64],
    pub search_mode: SearchMode,
}

impl Position {
    fn empty(board: Board, pull_mode: bool, search_mode: SearchMode) -> Self {
        Self {
            moves: Vec::new(),
            scores: Vec::new(),
            hashes: Vec::new(),
            place_in_positions: Vec::new(),
            move_deadlocked: Vec::new(),
            board,
            father: None,
            depth: 0,
            position_score: ScoreElement::default(),
            position_hash: 0,
            position_deadlocked: false,
            weight: 0,
            pull_mode,
            has_corral: false,
            hash_indicators: [0; 64],
            search_mode,
        }
    }

    fn set_hash_indicators(&mut self) {
        self.hash_indicators = [0; 64];
        for &hash in &self.hashes {
            let upper = (hash >> 52) as u32;
            self.hash_indicators[(upper >> 6) as usize] |= 1u64 << (upper & 0x3f);
        }
    }
}

pub struct PositionSet {
    pub positions: Vec<Position>,
    capacity: usize,
}

impl PositionSet {
    pub fn new(capacity: usize) -> Self {
        Self {
            positions: Vec::with_capacity(capacity),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    pub fn at_capacity(&self) -> bool {
        self.positions.len() >= self.capacity
    }

    pub fn position_is_solved(&self, ctx: &LevelContext, index: usize) -> bool {
        let p = &self.positions[index];
        if p.search_mode == SearchMode::DeadlockSearch {
            if p.pull_mode {
                return false;
            }
            return !work_left_in_deadlock_zone(&p.board);
        }
        p.board
            .is_solved(p.pull_mode, &ctx.level.initial, ctx.level.initial_sokoban)
    }

    #[allow(clippy::too_many_arguments)]
    fn set_scores_to_moves(
        &self,
        ctx: &LevelContext,
        board: &Board,
        moves: &[Move],
        pull_mode: bool,
        search_mode: SearchMode,
        hashes: &mut Vec<u64>,
        scores: &mut Vec<ScoreElement>,
    ) {
        hashes.clear();
        scores.clear();
        for mv in moves {
            let mut c = *board;
            apply_move(&mut c, mv, search_mode, &ctx.level);
            hashes.push(c.hash());
            scores.push(score_board(ctx, &c, pull_mode, search_mode));
        }
    }

    fn set_place_in_positions(&self, p: &mut Position, existing: usize) {
        p.place_in_positions = vec![None; p.moves.len()];

        let mut indicators = [0u64; 256];
        for &hash in &p.hashes {
            let upper = (hash >> 50) as u32;
            indicators[(upper >> 6) as usize] |= 1u64 << (upper & 0x3f);
        }

        for i in 0..existing {
            let other = &self.positions[i];
            let upper = (other.position_hash >> 50) as u32;
            if (indicators[(upper >> 6) as usize] >> (upper & 0x3f)) & 1 == 0 {
                continue;
            }
            for (j, &hash) in p.hashes.iter().enumerate() {
                if hash == other.position_hash {
                    p.place_in_positions[j] = Some(i);
                    if other.position_deadlocked {
                        p.move_deadlocked[j] = true;
                    }
                }
            }
        }
    }

    /// Initializes a position set with one root (`set_first_position`).
    pub fn set_first_position(
        &mut self,
        ctx: &LevelContext,
        input: &Board,
        pull_mode: bool,
        search_mode: SearchMode,
        helper: &mut Helper,
        deadlock: &mut DeadlockState,
    ) -> Result<usize> {
        let board = *input;
        let (moves, has_corral) =
            find_possible_moves(ctx, &board, pull_mode, search_mode, helper, deadlock)?;

        let mut p = Position::empty(board, pull_mode, search_mode);
        p.has_corral = has_corral;
        p.position_score = score_board(ctx, &board, pull_mode, search_mode);
        p.moves = moves;
        p.move_deadlocked = vec![false; p.moves.len()];
        p.position_hash = board.hash();
        self.set_scores_to_moves(
            ctx,
            &board,
            &p.moves,
            pull_mode,
            search_mode,
            &mut p.hashes,
            &mut p.scores,
        );
        p.set_hash_indicators();
        let existing = self.positions.len();
        self.set_place_in_positions(&mut p, existing);
        self.positions.push(p);

        if self.positions[0].moves.is_empty() && !self.position_is_solved(ctx, 0) {
            self.positions[0].position_deadlocked = true;
        }
        Ok(0)
    }

    /// Expands `node` through `son` and appends the new position.
    pub fn expand_position(
        &mut self,
        ctx: &LevelContext,
        node: usize,
        son: usize,
        pull_mode: bool,
        helper: &mut Helper,
        deadlock: &mut DeadlockState,
    ) -> Result<usize> {
        let expected_hash = self.positions[node].hashes[son];
        let search_mode = self.positions[node].search_mode;
        if self.find_hash_in_positions(expected_hash).is_some() {
            return Err(crate::error::SolveError::Internal(
                "expanding a known position".into(),
            ));
        }

        let mut board = self.positions[node].board;
        let mv = self.positions[node].moves[son];
        apply_move(&mut board, &mv, search_mode, &ctx.level);
        if board.hash() != expected_hash {
            return Err(crate::error::SolveError::Internal(
                "position hash mismatch".into(),
            ));
        }

        let (moves, has_corral) =
            find_possible_moves(ctx, &board, pull_mode, search_mode, helper, deadlock)?;

        let mut p = Position::empty(board, pull_mode, search_mode);
        p.father = Some(node);
        p.depth = self.positions[node].depth + 1;
        p.has_corral = has_corral;
        p.position_score = self.positions[node].scores[son];
        p.moves = moves;
        p.move_deadlocked = vec![false; p.moves.len()];
        p.weight = self.positions[node].weight + self.positions[node].moves[son].attr.weight as i32;
        self.set_scores_to_moves(
            ctx,
            &board,
            &p.moves,
            pull_mode,
            search_mode,
            &mut p.hashes,
            &mut p.scores,
        );
        p.set_hash_indicators();
        let existing = self.positions.len();
        self.set_place_in_positions(&mut p, existing);
        p.position_hash = expected_hash;
        self.positions.push(p);

        let new_index = self.positions.len() - 1;
        self.positions[node].place_in_positions[son] = Some(new_index);
        self.direct_other_sons_to_position(new_index);

        if self.all_sons_are_deadlocked(new_index) && !self.position_is_solved(ctx, new_index) {
            self.positions[new_index].position_deadlocked = true;
            self.mark_deadlocks(ctx);
        }

        Ok(new_index)
    }

    pub fn direct_other_sons_to_position(&mut self, pos: usize) {
        let hash = self.positions[pos].position_hash;
        let upper = (hash >> 52) as u32;
        let lower = upper & 0x3f;
        let upper = upper >> 6;

        for i in 0..self.positions.len() {
            let p = &mut self.positions[i];
            if (p.hash_indicators[upper as usize] >> lower) & 1 == 0 {
                continue;
            }
            for j in 0..p.hashes.len() {
                if p.hashes[j] != hash {
                    continue;
                }
                if p.place_in_positions[j].is_some() {
                    continue;
                }
                p.place_in_positions[j] = Some(pos);
            }
        }
    }

    fn all_sons_are_deadlocked(&self, p: usize) -> bool {
        let position = &self.positions[p];
        for i in 0..position.moves.len() {
            if position.move_deadlocked[i] {
                continue;
            }
            let Some(q) = position.place_in_positions[i] else {
                return false;
            };
            if self.positions[q].position_deadlocked {
                debug_assert!(false, "move_deadlocked bookkeeping error");
                continue;
            }
            return false;
        }
        true
    }

    pub fn mark_deadlocks(&mut self, ctx: &LevelContext) {
        loop {
            let mut changed = false;
            for i in 0..self.positions.len() {
                if self.positions[i].position_deadlocked {
                    continue;
                }
                for j in 0..self.positions[i].moves.len() {
                    if self.positions[i].move_deadlocked[j] {
                        continue;
                    }
                    if let Some(t) = self.positions[i].place_in_positions[j] {
                        if self.positions[t].position_deadlocked {
                            self.positions[i].move_deadlocked[j] = true;
                            changed = true;
                        }
                    }
                }

                if self.positions[i].move_deadlocked.iter().all(|dead| *dead)
                    && !self.position_is_solved(ctx, i)
                {
                    self.positions[i].position_deadlocked = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    pub fn set_leaf_as_deadlock(&mut self, ctx: &LevelContext, pos: usize, son: usize) {
        let hash = self.positions[pos].hashes[son];
        let upper = (hash >> 52) as u32;
        let lower = upper & 0x3f;
        let upper = (upper >> 6) as usize;
        let mut node_changed = false;

        for i in 0..self.positions.len() {
            if self.positions[i].position_deadlocked {
                continue;
            }
            if (self.positions[i].hash_indicators[upper] >> lower) & 1 == 0 {
                continue;
            }
            let n = self.positions[i].moves.len();
            for j in 0..n {
                if self.positions[i].hashes[j] != hash {
                    continue;
                }
                self.positions[i].move_deadlocked[j] = true;
                if i != pos {
                    debug_assert!(
                        self.positions[i].place_in_positions[j].is_none(),
                        "leaf should not be expanded"
                    );
                }
                if self.positions[i].move_deadlocked.iter().all(|dead| *dead) {
                    node_changed = true;
                }
            }
        }

        if node_changed {
            self.mark_deadlocks(ctx);
        }
    }

    pub fn find_hash_in_positions(&self, hash: u64) -> Option<usize> {
        self.positions
            .iter()
            .position(|position| position.position_hash == hash)
    }

    pub fn estimate_uniqe_moves(&self) -> usize {
        let mut seen = [false; 1024];
        let mut sum = 0;
        for position in &self.positions {
            for &hash in &position.hashes {
                seen[(hash & 0x3ff) as usize] = true;
            }
        }
        for value in seen {
            if value {
                sum += 1;
            }
        }
        sum
    }

    pub fn show_position_path(&self, mut p: Option<usize>) -> Vec<usize> {
        let mut path = Vec::new();
        while let Some(index) = p {
            path.push(index);
            p = self.positions[index].father;
        }
        path.reverse();
        path
    }
}
