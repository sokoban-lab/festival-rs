// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Per-thread search state (`helper.h`).
//!
//! The C++ helper also owns plan/oop/imagine buffers. Only the fields used by
//! the Phase 2 forward search live here; later phases extend this struct.

use crate::moves::Move;
use crate::search::features::RequestData;

#[derive(Debug)]
pub struct Helper {
    pub requests: RequestData,
    pub perimeter_found: bool,
    pub enable_inefficient_moves: bool,
    pub weighted_search: bool,
    pub level_solved: bool,
    pub sol_moves: Vec<Move>,
    pub alg_type: i32,
}

impl Default for Helper {
    fn default() -> Self {
        Self {
            requests: RequestData::default(),
            perimeter_found: false,
            enable_inefficient_moves: false,
            weighted_search: true,
            level_solved: false,
            sol_moves: Vec::with_capacity(crate::constants::MAX_SOL_LEN),
            alg_type: 0,
        }
    }
}

impl Helper {
    pub fn reset(&mut self) {
        self.requests.clear();
        self.perimeter_found = false;
        self.enable_inefficient_moves = false;
        self.weighted_search = true;
        self.level_solved = false;
        self.sol_moves.clear();
    }

    pub fn sol_len(&self) -> usize {
        self.sol_moves.len()
    }
}
