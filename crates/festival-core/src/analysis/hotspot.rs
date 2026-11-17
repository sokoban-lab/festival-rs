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

fn accumulate_pull_reachability(
    level: &Level,
    graph: &mut Graph,
    target_indices: &[usize],
    counts: &mut [u16],
) {
    counts.fill(0);
    let n = level.index_num();
    for &t in target_indices {
        counts[t] += 1;
        graph.set_weights_infinity();
        graph.clear_weight_around_cell(t);
        graph.do_iterations(true);
        for (p, slot) in counts.iter_mut().enumerate().take(n) {
            if p == t {
                continue;
            }
            let base = p * 4;
            if graph.vertices[base].weight != INF
                || graph.vertices[base + 1].weight != INF
                || graph.vertices[base + 2].weight != INF
                || graph.vertices[base + 3].weight != INF
            {
                *slot += 1;
            }
        }
    }
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
    let target_indices: Vec<usize> = level
        .coords
        .iter()
        .enumerate()
        .filter_map(|(idx, &(y, x))| {
            (board.cells()[y as usize * MAX_SIZE + x as usize] & TARGET != 0).then_some(idx)
        })
        .collect();

    // The pull graph on an otherwise-empty board is the exact transpose of the
    // push graph, so `target_indices.len()` reverse traversals replace `n`
    // forward traversals per blocker.
    graph.build(level, &empty, true);
    accumulate_pull_reachability(
        level,
        &mut graph,
        &target_indices,
        &mut hotspots.targets_reachable_from,
    );

    let mut updated_counts = vec![0u16; n];
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
        graph.build(level, &board_with_blocker, true);
        accumulate_pull_reachability(level, &mut graph, &target_indices, &mut updated_counts);

        for p in 0..n {
            if distances.impossible(p) || p == blocker {
                continue;
            }
            let (y, x) = level.coord(p);
            if board.cell(y, x) & TARGET != 0 {
                continue;
            }

            let updated_targets = updated_counts[p];
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

    #[test]
    fn transpose_init_matches_forward_push_reference() {
        for text in [
            "#########\n#@ $ $ .#\n#########\n",
            "##########\n#@  $    #\n#  ###   #\n#  $ $ ..#\n#   $   .#\n##########\n",
            "############\n#@  #     .#\n# $ # ###  #\n#   $   $ .#\n#  ###    .#\n############\n",
        ] {
            let level = level(text);
            let distances = distance::compute(&level);
            let actual = init(&level, &distances, None);

            let n = level.index_num();
            let words = n.div_ceil(64);
            let board = level.board;
            let empty = board.without_boxes();
            let mut graph = Graph::new(&level);
            let mut mask = Board::new(board.width(), board.height()).unwrap();
            let count_targets = |m: &Board| -> u16 {
                level
                    .coords
                    .iter()
                    .filter(|&&(y, x)| {
                        m.cell(y as usize, x as usize) != 0
                            && board.cell(y as usize, x as usize) & TARGET != 0
                    })
                    .count() as u16
            };

            let mut exp_reach = vec![0u16; n];
            graph.build(&level, &empty, false);
            for (p, &(y, x)) in level.coords.iter().enumerate() {
                graph.find_targets_of_place_without_init_graph(
                    &level,
                    y as usize,
                    x as usize,
                    &mut mask,
                );
                exp_reach[p] = count_targets(&mask);
            }

            let mut exp_blocks = vec![0u64; n * words];
            let mut exp_comp = vec![0u64; n * words];
            for blocker in 0..n {
                if distances.impossible(blocker) {
                    continue;
                }
                let (by, bx) = level.coord(blocker);
                if board.cell(by, bx) & TARGET != 0 {
                    continue;
                }
                let mut wb = empty;
                wb.set(by, bx, WALL);
                graph.build(&level, &wb, false);
                for p in 0..n {
                    if distances.impossible(p) || p == blocker {
                        continue;
                    }
                    let (py, px) = level.coord(p);
                    if board.cell(py, px) & TARGET != 0 {
                        continue;
                    }
                    graph.find_targets_of_place_without_init_graph(&level, py, px, &mut mask);
                    let upd = count_targets(&mask);
                    if upd != exp_reach[p] {
                        exp_blocks[blocker * words + (p >> 6)] |= 1u64 << (p & 63);
                    }
                    if upd == 0 {
                        exp_comp[p * words + (blocker >> 6)] |= 1u64 << (blocker & 63);
                    }
                }
            }

            assert_eq!(actual.targets_reachable_from, exp_reach);
            assert_eq!(actual.blocks_any_by_blocker, exp_blocks);
            assert_eq!(actual.completely_blocks, exp_comp);
        }
    }
}
