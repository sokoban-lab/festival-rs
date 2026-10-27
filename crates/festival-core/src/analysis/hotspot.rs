// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Hotspot table (`hotspot.cpp`).
//!
//! A hotspot is a box that blocks another box's way to any target.

use std::time::Instant;

use crate::board::Board;
use crate::constants::*;
use crate::distance::Distances;
use crate::graph::Graph;
use crate::level::Level;

pub struct Hotspots {
    pub targets_reachable_from: Vec<u16>,
    /// `blocks_place[p * MAX_INNER + blocker]`
    pub blocks_place: Vec<i32>,
    /// `completely_blocks[p * MAX_INNER + blocker]`
    pub completely_blocks: Vec<bool>,
    pub aborted: bool,
}

impl Hotspots {
    fn empty() -> Self {
        Self {
            targets_reachable_from: vec![0; MAX_INNER],
            blocks_place: vec![0; MAX_INNER * MAX_INNER],
            completely_blocks: vec![false; MAX_INNER * MAX_INNER],
            aborted: false,
        }
    }

    fn clear(&mut self) {
        self.targets_reachable_from.fill(0);
        self.blocks_place.fill(0);
        self.completely_blocks.fill(false);
    }

    pub fn double_blocking(&self, index1: usize, index2: usize) -> bool {
        self.completely_blocks[index1 * MAX_INNER + index2]
            && self.completely_blocks[index2 * MAX_INNER + index1]
    }

    pub fn find_hotspots(&self, level: &Level, board: &Board, h: &mut Board, weights: &mut Board) {
        h.zero();
        weights.zero();
        let mut boxes = Vec::with_capacity(level.index_num());

        for y in 0..board.height() {
            for x in 0..board.width() {
                if !level.is_inner(y, x) {
                    continue;
                }
                if board.cell(y, x) & BOX != 0 {
                    if let Some(index) = level.index_at(y, x) {
                        boxes.push(index);
                    }
                }
            }
        }

        for &blocker in &boxes {
            for &blocked in &boxes {
                if blocked == blocker {
                    continue;
                }
                if self.blocks_place[blocked * MAX_INNER + blocker] != 0 {
                    let (y, x) = level.coord(blocker);
                    h.set(y, x, 1);
                    weights.set(y, x, weights.cell(y, x) + 1);
                }
            }
        }
    }

    pub fn score(&self, level: &Level, board: &Board) -> usize {
        let mut h = Board::new(board.width(), board.height()).expect("same dims");
        let mut weights = Board::new(board.width(), board.height()).expect("same dims");
        self.find_hotspots(level, board, &mut h, &mut weights);
        h.cells()
            .iter()
            .take(MAX_INNER)
            .filter(|cell| **cell != 0)
            .count()
    }
}

fn count_targets_on_mask(level: &Level, board: &Board, mask: &Board) -> u16 {
    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if !level.is_inner(y, x) {
                continue;
            }
            if mask.cell(y, x) != 0 && board.cell(y, x) & TARGET != 0 {
                sum += 1;
            }
        }
    }
    sum
}

pub fn init(level: &Level, distances: &Distances, abort_deadline: Option<Instant>) -> Hotspots {
    let mut hotspots = Hotspots::empty();
    let n = level.index_num();
    let board = level.board;
    let empty = board.without_boxes();
    let mut graph = Graph::new(level);
    let mut targets = Board::new(board.width(), board.height()).expect("same dims");

    graph.build(level, &empty, false);
    for p in 0..n {
        let (y, x) = level.coord(p);
        graph.find_targets_of_place_without_init_graph(level, y, x, &mut targets);
        hotspots.targets_reachable_from[p] = count_targets_on_mask(level, &board, &targets);
    }

    for blocker in 0..n {
        if let Some(deadline) = abort_deadline {
            if Instant::now() >= deadline {
                hotspots.clear();
                hotspots.aborted = true;
                return hotspots;
            }
        }

        if distances.impossible(blocker) {
            continue;
        }
        let (blocker_y, blocker_x) = level.coord(blocker);
        if board.cell(blocker_y, blocker_x) & TARGET != 0 {
            continue;
        }

        let mut board_with_blocker = empty;
        board_with_blocker.set(blocker_y, blocker_x, WALL);
        graph.build(level, &board_with_blocker, false);

        for p in 0..n {
            if distances.impossible(p) || p == blocker {
                continue;
            }
            let (y, x) = level.coord(p);
            if board.cell(y, x) & TARGET != 0 {
                continue;
            }

            graph.find_targets_of_place_without_init_graph(level, y, x, &mut targets);
            let updated_targets = count_targets_on_mask(level, &board, &targets);
            let idx = p * MAX_INNER + blocker;
            hotspots.blocks_place[idx] =
                hotspots.targets_reachable_from[p] as i32 - updated_targets as i32;
            if updated_targets == 0 {
                hotspots.completely_blocks[idx] = true;
            }
        }
    }

    hotspots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distance;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn self_blocking_is_detected() {
        let level = level("#########\n#@ $ $ .#\n#########\n");
        let distances = distance::compute(&level);
        let hotspots = init(&level, &distances, None);
        assert!(!hotspots.blocks_place.is_empty());
        assert!(!hotspots.aborted);
    }
}
