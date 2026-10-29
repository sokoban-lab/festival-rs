//! Per-thread search state (`helper.h`).
//!
//! Owns the request ring, the solution buffer and the Phase 4 planning data
//! (parking order, OOP zones, imagined boards, per-search scored distance).

use crate::board::Board;
use crate::constants::{INF, MAX_INNER};
use crate::graph::Graph;
use crate::moves::Move;
use crate::planning::{ImagineData, OopData, ParkOrder};
use crate::search::features::RequestData;
use crate::solver::LevelContext;

#[derive(Debug)]
pub struct Helper {
    pub requests: RequestData,
    pub perimeter_found: bool,
    pub enable_inefficient_moves: bool,
    pub weighted_search: bool,
    pub level_solved: bool,
    pub sol_moves: Vec<Move>,
    pub alg_type: i32,
    pub used_in_plan: Board,
    pub imagined_hf_board: Board,
    pub wallified: Board,
    pub k_dist_value: i32,
    pub tested_deadlock_room: usize,

    /// Push or pull distance from each cell to the relevant targets. The
    /// direction is fixed at the start of each search (`init_scored_distance`).
    pub scored_distance: [u32; MAX_INNER],
    pub parking_order: Vec<ParkOrder>,
    pub full_parking_order: Vec<ParkOrder>,
    pub boxes_were_removed: bool,
    pub oop: Option<OopData>,
    pub imagine: Option<ImagineData>,

    /// Reused `find_possible_moves` buffers (moves scratch and box graph).
    pub move_scratch: Vec<Move>,
    pub graph_scratch: Option<Graph>,
}

impl Default for Helper {
    fn default() -> Self {
        let zero = Board::new(crate::constants::MAX_SIZE, crate::constants::MAX_SIZE)
            .expect("MAX_SIZE board");
        Self {
            requests: RequestData::default(),
            perimeter_found: false,
            enable_inefficient_moves: false,
            weighted_search: true,
            level_solved: false,
            sol_moves: Vec::with_capacity(crate::constants::MAX_SOL_LEN),
            alg_type: 0,
            used_in_plan: zero,
            imagined_hf_board: zero,
            wallified: zero,
            k_dist_value: 0,
            tested_deadlock_room: 0,
            scored_distance: [INF; MAX_INNER],
            parking_order: Vec::new(),
            full_parking_order: Vec::new(),
            boxes_were_removed: false,
            oop: None,
            imagine: None,
            move_scratch: Vec::new(),
            graph_scratch: None,
        }
    }
}

impl Helper {
    /// Clears everything C++ `reset_helper` clears, plus the planning data.
    pub fn reset(&mut self) {
        self.requests.clear();
        self.perimeter_found = false;
        self.enable_inefficient_moves = false;
        self.weighted_search = true;
        self.level_solved = false;
        self.sol_moves.clear();
        self.used_in_plan.zero();
        self.imagined_hf_board.zero();
        self.wallified.zero();
        self.k_dist_value = 0;
        self.tested_deadlock_room = 0;
        self.parking_order.clear();
        self.full_parking_order.clear();
        self.boxes_were_removed = false;
        self.oop = None;
        self.imagine = None;
    }

    /// Per-search reset. Planning data survives between the reverse and
    /// forward phases of one strategy, but not between strategies.
    pub fn begin_search(&mut self) {
        self.requests.clear();
        self.perimeter_found = false;
        self.enable_inefficient_moves = false;
        self.level_solved = false;
        self.sol_moves.clear();
    }

    /// Port of `init_scored_distance`.
    pub fn init_scored_distance(&mut self, ctx: &LevelContext, board: &Board, pull_mode: bool) {
        if !pull_mode {
            crate::distance::get_distance_from_targets(
                &ctx.level,
                board,
                &mut self.scored_distance,
            );
            return;
        }

        self.scored_distance = [0; MAX_INNER];
        let n = ctx.level.index_num();
        for from in 0..n {
            let (from_y, from_x) = ctx.level.coord(from);
            for to in 0..n {
                let (to_y, to_x) = ctx.level.coord(to);
                if board.cell(to_y, to_x) & crate::constants::TARGET == 0 {
                    continue;
                }
                let dist = ctx.distances.from_to(from, to);
                if dist != INF {
                    self.scored_distance[Board::flat_index(from_y, from_x)] += dist;
                }
            }
        }
    }

    pub fn sol_len(&self) -> usize {
        self.sol_moves.len()
    }
}
