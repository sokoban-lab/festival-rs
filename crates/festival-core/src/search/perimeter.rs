//! Search frontier (`perimeter.cpp`).
//!
//! C++ protects the global perimeter table with a single mutex across all
//! worker threads. Using a single `RwLock<FastMap>` paired with a lock-free
//! `AtomicUsize` entry counter lets `is_full()` avoid locking altogether and
//! lets multi-core `for_each_hit` / Dragonfly reads proceed concurrently
//! without multi-shard lock fan-out overhead.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use crate::constants::SearchMode;
use crate::moves::apply_move;
use crate::search::tree::Tree;
use crate::solver::LevelContext;

/// Fast deterministic mixer for keys that are already 64-bit board hashes.
#[derive(Default)]
struct U64Hasher(u64);

impl Hasher for U64Hasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.wrapping_mul(0x100000001b3) ^ (b as u64);
        }
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        let mut x = i;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        self.0 = x ^ (x >> 31);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<U64Hasher>>;

#[derive(Clone, Copy, Debug)]
struct Entry {
    depth: u16,
    side: u8,
    alg: u8,
}

#[derive(Debug, Default)]
struct Table {
    entries: RwLock<FastMap<u64, Entry>>,
    total_entries: AtomicUsize,
}

#[derive(Debug)]
pub struct Perimeter {
    base: Option<Arc<Table>>,
    table: Arc<Table>,
    log_size: u32,
}

impl Perimeter {
    pub fn new(log_size: u32) -> Self {
        Self {
            base: None,
            table: Arc::new(Table::default()),
            log_size,
        }
    }

    pub fn clear(&self) {
        let mut entries = self.table.entries.write().expect("perimeter write lock");
        entries.clear();
        self.table.total_entries.store(0, Ordering::Relaxed);
    }

    /// Copies the current entries into an independent perimeter.
    pub fn snapshot(&self) -> Self {
        let mut merged = match &self.base {
            Some(base) => base
                .entries
                .read()
                .expect("perimeter base read lock")
                .clone(),
            None => FastMap::default(),
        };
        let local = self.table.entries.read().expect("perimeter read lock");
        if merged.is_empty() {
            merged = local.clone();
        } else {
            for (&hash, &entry) in local.iter() {
                Self::upsert_map(&mut merged, hash, entry.depth, entry.side, entry.alg);
            }
        }
        let len = merged.len();
        Self {
            base: None,
            table: Arc::new(Table {
                entries: RwLock::new(merged),
                total_entries: AtomicUsize::new(len),
            }),
            log_size: self.log_size,
        }
    }

    /// Creates a zero-copy write-isolated overlay over `self` for short
    /// portfolio probes, avoiding cloning the reverse frontier table.
    pub fn overlay(&self) -> Self {
        let root = match &self.base {
            Some(base) => Arc::clone(base),
            None => Arc::clone(&self.table),
        };
        Self {
            base: Some(root),
            table: Arc::new(Table::default()),
            log_size: self.log_size,
        }
    }

    /// Flushes this overlay's local writes into `target`.
    pub fn commit_to(&self, target: &Self) {
        let local = self.table.entries.read().expect("perimeter read lock");
        if local.is_empty() {
            return;
        }
        let mut dst = target
            .table
            .entries
            .write()
            .expect("perimeter target write lock");
        let mut added = 0usize;
        for (&hash, &entry) in local.iter() {
            if Self::upsert_map(&mut dst, hash, entry.depth, entry.side, entry.alg) {
                added += 1;
            }
        }
        if added > 0 {
            target.table.total_entries.fetch_add(added, Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn total_entries(&self) -> usize {
        let base_count = self
            .base
            .as_ref()
            .map_or(0, |b| b.total_entries.load(Ordering::Relaxed));
        base_count + self.table.total_entries.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        let limit = 1usize << (self.log_size - 1);
        self.total_entries() >= limit
    }

    #[inline]
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

    #[inline]
    fn upsert_map(map: &mut FastMap<u64, Entry>, hash: u64, depth: u16, side: u8, alg: u8) -> bool {
        if let Some(entry) = map.get_mut(&hash) {
            Self::update(entry, depth, side, alg);
            return false;
        }
        map.insert(hash, Entry { depth, side, alg });
        true
    }

    #[cfg(test)]
    fn store(&self, hash: u64, depth: u16, side: u8, alg: u8) {
        let mut map = self.table.entries.write().expect("perimeter write lock");
        if Self::upsert_map(&mut map, hash, depth, side, alg) {
            self.table.total_entries.fetch_add(1, Ordering::Relaxed);
        }
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
            let move_hash = tree.move_hashes[e.move_hash_place + son];
            if tree.nodes[move_hash.node as usize].score.boxes_in_level
                != ctx.level.box_count as i16
            {
                return u64::MAX;
            }
            apply_move(
                &mut board,
                &move_hash.move_data,
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
        &self,
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

        let moves_num = usize::from(with_sons) * e.moves_num as usize;
        if moves_num < 256 {
            let mut stack_entries = [(0u64, 0u16); 256];
            stack_entries[0] = (node_hash, e.depth as u16);
            let mut len = 1usize;
            if with_sons {
                let start = e.move_hash_place;
                let son_depth = (e.depth + 1) as u16;
                for offset in 0..e.moves_num as usize {
                    let hash = if tree.search_mode == SearchMode::BaseSearch {
                        Self::hash_without_bases(ctx, tree, expansion, Some(offset))
                    } else {
                        tree.move_hashes[start + offset].hash
                    };
                    stack_entries[len] = (hash, son_depth);
                    len += 1;
                }
            }
            let mut map = self.table.entries.write().expect("perimeter write lock");
            let mut added = 0usize;
            for &(hash, depth) in &stack_entries[..len] {
                if Self::upsert_map(&mut map, hash, depth, side, alg) {
                    added += 1;
                }
            }
            if added > 0 {
                self.table.total_entries.fetch_add(added, Ordering::Relaxed);
            }
            return len;
        }

        let mut entries = Vec::with_capacity(1 + moves_num);
        entries.push((node_hash, e.depth as u16));
        if with_sons {
            let start = e.move_hash_place;
            for offset in 0..e.moves_num as usize {
                let hash = if tree.search_mode == SearchMode::BaseSearch {
                    Self::hash_without_bases(ctx, tree, expansion, Some(offset))
                } else {
                    tree.move_hashes[start + offset].hash
                };
                entries.push((hash, (e.depth + 1) as u16));
            }
        }

        let mut map = self.table.entries.write().expect("perimeter write lock");
        let mut added = 0usize;
        for &(hash, depth) in &entries {
            if Self::upsert_map(&mut map, hash, depth, side, alg) {
                added += 1;
            }
        }
        if added > 0 {
            self.table.total_entries.fetch_add(added, Ordering::Relaxed);
        }
        entries.len()
    }

    #[inline]
    fn lookup_in_maps(
        base: Option<&FastMap<u64, Entry>>,
        local: &FastMap<u64, Entry>,
        hash: u64,
        side: u8,
    ) -> Option<u16> {
        let merged = match (base.and_then(|b| b.get(&hash)), local.get(&hash)) {
            (Some(&b_ent), Some(&l_ent)) => {
                let mut e = b_ent;
                Self::update(&mut e, l_ent.depth, l_ent.side, l_ent.alg);
                Some(e)
            }
            (None, Some(&l_ent)) => Some(l_ent),
            (Some(&b_ent), None) => Some(b_ent),
            (None, None) => None,
        };
        merged
            .filter(|entry| entry.side == side)
            .map(|entry| entry.depth)
    }

    /// Returns the stored depth when `hash` is on `side`.
    pub fn is_in(&self, hash: u64, side: u8) -> Option<u16> {
        let local = self.table.entries.read().expect("perimeter read lock");
        if let Some(base) = &self.base {
            let b = base.entries.read().expect("perimeter base read lock");
            Self::lookup_in_maps(Some(&b), &local, hash, side)
        } else {
            local
                .get(&hash)
                .filter(|entry| entry.side == side)
                .map(|entry| entry.depth)
        }
    }

    /// Evaluates all son hashes of an expansion under a single shared read
    /// lock, calling `accept(son_idx, depth)` on hits.
    pub fn for_each_hit(
        &self,
        sons: &[crate::search::tree::MoveHashData],
        side: u8,
        mut accept: impl FnMut(usize, u16),
    ) {
        if sons.is_empty() {
            return;
        }
        let local = self.table.entries.read().expect("perimeter read lock");
        if let Some(base) = &self.base {
            let b = base.entries.read().expect("perimeter base read lock");
            for (i, mh) in sons.iter().enumerate() {
                if let Some(depth) = Self::lookup_in_maps(Some(&b), &local, mh.hash, side) {
                    accept(i, depth);
                }
            }
        } else {
            if local.is_empty() {
                return;
            }
            for (i, mh) in sons.iter().enumerate() {
                if let Some(entry) = local.get(&mh.hash) {
                    if entry.side == side {
                        accept(i, entry.depth);
                    }
                }
            }
        }
    }

    /// Writes dragonfly visitation markers into `out` under a single shared
    /// read lock without allocating.
    pub fn mark_dragonfly_visited_into(&self, hashes: &[u64], out: &mut Vec<i32>) {
        out.clear();
        out.resize(hashes.len(), 0);
        if hashes.is_empty() {
            return;
        }
        let map = self.table.entries.read().expect("perimeter read lock");
        if map.is_empty() {
            return;
        }
        for (i, &hash) in hashes.iter().enumerate() {
            if let Some(entry) = map.get(&hash) {
                out[i] = if entry.alg == SearchMode::Dragonfly as u8 {
                    1
                } else if entry.side == 0 {
                    -2000 + entry.depth as i32
                } else {
                    0
                };
            }
        }
    }

    /// `dragonfly_mark_visited_hashes`: `1` for an earlier dragonfly visit,
    /// a negative attractive distance for a forward-frontier hit, else `0`.
    pub fn mark_dragonfly_visited(&self, hashes: &[u64]) -> Vec<i32> {
        let mut out = Vec::with_capacity(hashes.len());
        self.mark_dragonfly_visited_into(hashes, &mut out);
        out
    }

    /// `dragonfly_update_perimeter`.
    pub fn update_dragonfly(&self, hashes: &[u64], visited: &[i32], depth: u16) {
        if hashes.is_empty() {
            return;
        }
        let alg = SearchMode::Dragonfly as u8;
        let mut map = self.table.entries.write().expect("perimeter write lock");
        let mut added = 0usize;
        for (index, &hash) in hashes.iter().enumerate() {
            if visited.get(index).copied().unwrap_or(0) == 1 {
                continue;
            }

            if let Some(entry) = map.get_mut(&hash) {
                if entry.side == 1 {
                    if depth < entry.depth {
                        entry.depth = depth;
                    }
                    entry.alg = alg;
                    continue;
                }
                entry.depth = depth;
                entry.side = 1;
                entry.alg = alg;
                continue;
            }

            map.insert(
                hash,
                Entry {
                    depth,
                    side: 1,
                    alg,
                },
            );
            added += 1;
        }
        if added > 0 {
            self.table.total_entries.fetch_add(added, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::constants::{MAX_SIZE, SearchMode};
    use crate::level::Level;

    fn dummy_tree() -> Tree {
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
        let perimeter = Perimeter::new(8);
        perimeter.store(7, 4, 0, 0);
        perimeter.store(7, 6, 1, 0);
        assert_eq!(perimeter.is_in(7, 1), Some(6));
        assert_eq!(perimeter.is_in(7, 0), None);
    }

    #[test]
    fn shallower_depth_updates() {
        let perimeter = Perimeter::new(8);
        perimeter.store(9, 5, 0, 0);
        perimeter.store(9, 2, 0, 0);
        assert_eq!(perimeter.is_in(9, 0), Some(2));
    }

    #[test]
    fn concurrent_access_is_synchronized() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let perimeter = Arc::new(Perimeter::new(10));
        let solved = Arc::new(AtomicBool::new(false));
        std::thread::scope(|scope| {
            for thread in 0..4u64 {
                let perimeter = Arc::clone(&perimeter);
                let solved = Arc::clone(&solved);
                scope.spawn(move || {
                    for i in 0..500u64 {
                        let hash = i * 8 + thread;
                        perimeter.store(hash, 3, 0, 0);
                        assert_eq!(perimeter.is_in(hash, 0), Some(3));
                        if i == 250 {
                            solved.store(true, Ordering::Relaxed);
                        }
                        let _ = solved.load(Ordering::Relaxed);
                    }
                });
            }
        });
        assert_eq!(perimeter.total_entries(), 2000);
    }

    #[test]
    fn dummy_tree_compiles() {
        assert!(dummy_tree().expansions.is_empty());
    }

    #[test]
    fn overlay_isolates_probes_until_committed() {
        let base = Perimeter::new(10);
        base.store(100, 5, 1, 0);
        let ov1 = base.overlay();
        let ov2 = base.overlay();

        ov1.store(200, 3, 0, 0);
        assert_eq!(ov1.is_in(100, 1), Some(5));
        assert_eq!(ov1.is_in(200, 0), Some(3));
        assert_eq!(ov2.is_in(200, 0), None);
        assert_eq!(base.is_in(200, 0), None);

        ov1.commit_to(&base);
        assert_eq!(base.is_in(200, 0), Some(3));
        assert_eq!(base.total_entries(), 2);
    }
}
