// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Search frontier (`perimeter.cpp`).
//!
//! The C++ version uses a fixed open-addressed table shared by cores. Phase 2
//! runs a single forward search, so a hash map with the same entry/update
//! semantics is enough; the table moves behind `SharedRun` in Phase 6.

use std::collections::HashMap;

use crate::constants::SearchMode;
use crate::moves::apply_move;
use crate::search::tree::Tree;
use crate::solver::LevelContext;

#[derive(Clone, Copy, Debug)]
struct Entry {
    depth: u16,
    side: u8,
    alg: u8,
}

#[derive(Debug)]
pub struct Perimeter {
    entries: HashMap<u64, Entry>,
    log_size: u32,
    total_entries: usize,
}

impl Perimeter {
    pub fn new(log_size: u32) -> Self {
        Self {
            entries: HashMap::new(),
            log_size,
            total_entries: 0,
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_entries = 0;
    }

    pub fn total_entries(&self) -> usize {
        self.total_entries
    }

    pub fn is_full(&self) -> bool {
        let limit = 1usize << (self.log_size - 1);
        self.total_entries >= limit
    }

    fn update(entry: &mut Entry, depth: u16, side: u8, alg: u8) {
        // Dragonfly nodes must not be explored twice; only depth may improve.
        if entry.alg == SearchMode::Dragonfly as u8 {
            if side == 1 && depth < entry.depth {
                entry.depth = depth;
            }
            return;
        }

        if entry.side == side {
            if depth < entry.depth {
                entry.depth = depth;
                entry.alg = alg;
            }
            return;
        }

        // Different directions: pull side wins.
        if entry.side == 1 {
            return;
        }
        entry.side = side;
        entry.depth = depth;
        entry.alg = alg;
    }

    fn store(&mut self, hash: u64, depth: u16, side: u8, alg: u8) {
        if let Some(entry) = self.entries.get_mut(&hash) {
            Self::update(entry, depth, side, alg);
            return;
        }
        self.entries.insert(hash, Entry { depth, side, alg });
        self.total_entries += 1;
    }

    /// Port of `get_hash_without_bases`: BASE_SEARCH perimeter entries ignore
    /// the base markers so they can meet the forward search's plain boards.
    fn hash_without_bases(
        ctx: &LevelContext,
        tree: &Tree,
        expansion: u32,
        son: Option<usize>,
    ) -> u64 {
        let e = &tree.expansions[expansion as usize];
        if tree.nodes[e.node as usize].score.boxes_in_level != ctx.level.box_count as i16 {
            return u64::MAX;
        }

        let mut board = match tree.expansion_board(expansion) {
            Ok(board) => board,
            Err(_) => return u64::MAX,
        };

        if let Some(son) = son {
            let hash = tree.move_hashes[e.move_hash_place + son].hash;
            let Some(place) = tree.find_node_by_hash(hash) else {
                return u64::MAX;
            };
            if tree.nodes[place as usize].score.boxes_in_level != ctx.level.box_count as i16 {
                return u64::MAX;
            }
            apply_move(
                &mut board,
                &tree.move_hashes[e.move_hash_place + son].move_data,
                SearchMode::Normal,
                &ctx.level,
            );
        }

        board.clear_bases_inplace();
        board.hash()
    }

    /// Port of `insert_node_into_perimeter`; returns the number of inserted
    /// entries (including updates), like the C++ counter.
    pub fn insert_node(
        &mut self,
        ctx: &LevelContext,
        tree: &Tree,
        expansion: u32,
        with_sons: bool,
    ) -> usize {
        if self.is_full() {
            return 0;
        }

        let e = &tree.expansions[expansion as usize];
        let node_hash = if tree.search_mode == SearchMode::BaseSearch {
            Self::hash_without_bases(ctx, tree, expansion, None)
        } else {
            tree.nodes[e.node as usize].hash
        };
        let side = tree.pull_mode as u8;
        let alg = tree.search_mode as u8;

        let mut count = 0;
        self.store(node_hash, e.depth as u16, side, alg);
        count += 1;

        if !with_sons {
            return count;
        }

        let start = e.move_hash_place;
        for offset in 0..e.moves_num as usize {
            let hash = if tree.search_mode == SearchMode::BaseSearch {
                Self::hash_without_bases(ctx, tree, expansion, Some(offset))
            } else {
                tree.move_hashes[start + offset].hash
            };
            self.store(hash, (e.depth + 1) as u16, side, alg);
            count += 1;
        }
        count
    }

    /// Returns the stored depth when `hash` is on `side`.
    pub fn is_in(&self, hash: u64, side: u8) -> Option<u16> {
        self.entries
            .get(&hash)
            .filter(|entry| entry.side == side)
            .map(|entry| entry.depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::constants::{MAX_SIZE, SearchMode};
    use crate::level::Level;

    fn dummy_tree() -> Tree {
        // Width/height only matter for board storage; this tree never scores.
        let board = Board::new(3, 3).unwrap();
        let level = Level {
            board,
            initial: board,
            title: String::new(),
            inner: [false; crate::constants::MAX_INNER],
            index_of: [-1; crate::constants::MAX_INNER],
            coords: Vec::new(),
            initial_sokoban: (0, 0),
            box_count: 0,
            target_holes: Board::new(3, 3).unwrap(),
            semi_holes: Board::new(3, 3).unwrap(),
            forbidden_push: Board::new(3, 3).unwrap(),
            forbidden_pull: Board::new(3, 3).unwrap(),
        };
        let mut tree = Tree::new_with_level(8, &level).unwrap();
        tree.pull_mode = false;
        tree.search_mode = SearchMode::Normal;
        let _ = MAX_SIZE;
        tree
    }

    #[test]
    fn pull_side_wins_perimeter() {
        let mut perimeter = Perimeter::new(8);
        perimeter.store(7, 4, 0, 0);
        perimeter.store(7, 6, 1, 0);
        assert_eq!(perimeter.is_in(7, 1), Some(6));
        assert_eq!(perimeter.is_in(7, 0), None);
    }

    #[test]
    fn shallower_depth_updates() {
        let mut perimeter = Perimeter::new(8);
        perimeter.store(9, 5, 0, 0);
        perimeter.store(9, 2, 0, 0);
        assert_eq!(perimeter.is_in(9, 0), Some(2));
    }

    #[test]
    fn dummy_tree_compiles() {
        assert!(dummy_tree().expansions.is_empty());
    }
}
