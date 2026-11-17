//! Hotspot table (`hotspot.cpp`).
//!
//! A hotspot is a box that blocks another box's way to any target.

use std::time::Instant;

use crate::board::Board;
use crate::constants::*;
use crate::distance::Distances;
use crate::graph::Graph;
use crate::level::Level;

const MAX_WORDS: usize = MAX_INNER.div_ceil(64);

pub struct Hotspots {
    pub n: usize,
    pub words: usize,
    pub targets_reachable_from: Vec<u16>,
    /// Row `blocker` (`blocker * words .. (blocker + 1) * words`) has bit `p` set
    /// iff a box at `blocker` reduces the number of targets reachable from `p` (`p != blocker`).
    pub blocks_any_by_blocker: Vec<u64>,
    /// Row `p` (`p * words .. (p + 1) * words`) has bit `blocker` set
    /// iff a box at `blocker` leaves `0` targets reachable from `p`.
    pub completely_blocks: Vec<u64>,
    pub has_any_blocker: bool,
    pub aborted: bool,
}

impl Hotspots {
    fn with_places(n: usize) -> Self {
        let words = n.div_ceil(64);
        Self {
            n,
            words,
            targets_reachable_from: vec![0; n],
            blocks_any_by_blocker: vec![0; n * words],
            completely_blocks: vec![0; n * words],
            has_any_blocker: false,
            aborted: false,
        }
    }

    fn clear(&mut self) {
        self.targets_reachable_from.fill(0);
        self.blocks_any_by_blocker.fill(0);
        self.completely_blocks.fill(0);
        self.has_any_blocker = false;
    }

    #[inline]
    pub fn double_blocking(&self, index1: usize, index2: usize) -> bool {
        if index1 >= self.n || index2 >= self.n {
            return false;
        }
        let c12 =
            (self.completely_blocks[index1 * self.words + (index2 >> 6)] >> (index2 & 63)) & 1 != 0;
        let c21 =
            (self.completely_blocks[index2 * self.words + (index1 >> 6)] >> (index1 & 63)) & 1 != 0;
        c12 && c21
    }

    fn collect_box_bits(&self, level: &Level, board: &Board, box_bits: &mut [u64; MAX_WORDS]) {
        let cells = board.cells();
        for (index, &(y, x)) in level.coords.iter().enumerate() {
            if cells[y as usize * MAX_SIZE + x as usize] & BOX != 0 {
                box_bits[index >> 6] |= 1u64 << (index & 63);
            }
        }
    }

    pub fn find_hotspots(&self, level: &Level, board: &Board, h: &mut Board, weights: &mut Board) {
        h.zero();
        weights.zero();
        if !self.has_any_blocker || self.words == 0 {
            return;
        }

        let words = self.words;
        let mut box_bits = [0u64; MAX_WORDS];
        self.collect_box_bits(level, board, &mut box_bits);

        for (word_idx, &word) in box_bits.iter().enumerate().take(words) {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let blocker = word_idx * 64 + bit;
                w &= w - 1;

                let row = &self.blocks_any_by_blocker[blocker * words..(blocker + 1) * words];
                let mut blocked_count = 0u32;
                for k in 0..words {
                    blocked_count += (row[k] & box_bits[k]).count_ones();
                }
                if blocked_count != 0 {
                    let (y, x) = level.coord(blocker);
                    h.set(y, x, 1);
                    weights.set(y, x, (blocked_count & 0xff) as u8);
                }
            }
        }
    }

    pub fn score(&self, level: &Level, board: &Board) -> usize {
        if !self.has_any_blocker || self.words == 0 {
            return 0;
        }

        let words = self.words;
        let mut box_bits = [0u64; MAX_WORDS];
        self.collect_box_bits(level, board, &mut box_bits);

        let mut count = 0usize;
        for (word_idx, &word) in box_bits.iter().enumerate().take(words) {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let blocker = word_idx * 64 + bit;
                w &= w - 1;

                let row = &self.blocks_any_by_blocker[blocker * words..(blocker + 1) * words];
                for k in 0..words {
                    if (row[k] & box_bits[k]) != 0 {
                        count += 1;
                        break;
                    }
                }
            }
        }
        count
    }
}

fn count_targets_on_mask(target_flats: &[usize], mask: &Board) -> u16 {
    let mask_cells = mask.cells();
    let mut sum = 0u16;
    for &flat in target_flats {
        if mask_cells[flat] != 0 {
            sum += 1;
        }
    }
    sum
}

pub fn init(level: &Level, distances: &Distances, abort_deadline: Option<Instant>) -> Hotspots {
    let n = level.index_num();
    let mut hotspots = Hotspots::with_places(n);
    if n == 0 {
        return hotspots;
    }

    let words = hotspots.words;
    let board = level.board;
    let empty = board.without_boxes();
    let mut graph = Graph::new(level);
    let mut targets = Board::new(board.width(), board.height()).expect("same dims");
    let target_flats: Vec<usize> = level
        .coords
        .iter()
        .map(|&(y, x)| y as usize * MAX_SIZE + x as usize)
        .filter(|&flat| board.cells()[flat] & TARGET != 0)
        .collect();

    graph.build(level, &empty, false);
    for (p, &(y, x)) in level.coords.iter().enumerate() {
        graph.find_targets_of_place_without_init_graph(level, y as usize, x as usize, &mut targets);
        hotspots.targets_reachable_from[p] = count_targets_on_mask(&target_flats, &targets);
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
            let updated_targets = count_targets_on_mask(&target_flats, &targets);
            if updated_targets != hotspots.targets_reachable_from[p] {
                hotspots.blocks_any_by_blocker[blocker * words + (p >> 6)] |= 1u64 << (p & 63);
                hotspots.has_any_blocker = true;
            }
            if updated_targets == 0 {
                hotspots.completely_blocks[p * words + (blocker >> 6)] |= 1u64 << (blocker & 63);
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
        assert!(hotspots.has_any_blocker);
        assert!(!hotspots.aborted);
        assert_eq!(hotspots.score(&level, &level.board), 1);
    }

    #[test]
    fn bitset_score_matches_find_hotspots_board_mask() {
        let level = level(
            "##########\n\
             #@  $    #\n\
             #  ###   #\n\
             #  $ $ ..#\n\
             #   $   .#\n\
             ##########\n",
        );
        let distances = distance::compute(&level);
        let hotspots = init(&level, &distances, None);

        let mut state = 0x1234_5678_9abc_def0u64;
        let mut h = Board::new(level.board.width(), level.board.height()).unwrap();
        let mut weights = Board::new(level.board.width(), level.board.height()).unwrap();

        for _ in 0..200 {
            let mut b = level.board.without_boxes();
            for &(y, x) in &level.coords {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                if (state >> 61) == 0 {
                    b.or(y as usize, x as usize, BOX);
                }
            }
            hotspots.find_hotspots(&level, &b, &mut h, &mut weights);
            let mask_count = h.cells().iter().take(MAX_INNER).filter(|&&c| c != 0).count();
            assert_eq!(hotspots.score(&level, &b), mask_count);
        }
    }
}
