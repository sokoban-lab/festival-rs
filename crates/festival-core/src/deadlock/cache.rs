// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Shared deadlock result cache (`deadlock_cache.cpp`).

use crate::constants::SearchMode;

const NOT_DEADLOCK: i8 = 0;
const IS_DEADLOCK: i8 = 1;
const SEARCH_EXCEEDED: i8 = 2;

#[derive(Clone, Copy, Default)]
struct CacheEntry {
    hash: u64,
    queries: u32,
    /// Bumped on `clear`; entries from an older generation read as empty, so a
    /// per-level reset never touches the 4M-entry table.
    generation: u32,
    result: i8,
    pull_mode: i8,
    alg: i8,
}

pub struct DeadlockCache {
    entries: Vec<CacheEntry>,
    mask: u64,
    total: usize,
    log_size: u32,
    generation: u32,
}

impl DeadlockCache {
    pub fn new(log_size: u32) -> Self {
        let log_size = log_size.clamp(10, 26);
        let size = 1usize << log_size;
        Self {
            entries: vec![CacheEntry::default(); size],
            mask: (size - 1) as u64,
            total: 0,
            log_size,
            generation: 1,
        }
    }

    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.entries.fill(CacheEntry::default());
            self.generation = 1;
        }
        self.total = 0;
    }

    #[inline]
    pub fn log_size(&self) -> u32 {
        self.log_size
    }

    fn find(&self, hash: u64, pull_mode: bool, alg: SearchMode) -> (usize, bool) {
        let mut index = (hash & self.mask) as usize;
        let pull = pull_mode as i8;
        let alg = alg as i8;
        loop {
            let entry = &self.entries[index];
            if entry.hash == 0 || entry.generation != self.generation {
                return (index, false);
            }
            if entry.hash == hash && entry.pull_mode == pull && entry.alg == alg {
                return (index, true);
            }
            index = (index + 1) & self.mask as usize;
        }
    }

    pub fn get(&self, hash: u64, pull_mode: bool, alg: SearchMode) -> Option<i8> {
        let (index, matched) = self.find(hash, pull_mode, alg);
        if matched {
            Some(self.entries[index].result)
        } else {
            None
        }
    }

    fn is_full(&self) -> bool {
        self.total >= (1usize << (self.log_size - 1))
    }

    pub fn insert(&mut self, hash: u64, result: i8, pull_mode: bool, alg: SearchMode) {
        if self.is_full() {
            return;
        }
        let pull = pull_mode as i8;
        let alg_id = alg as i8;
        let (index, matched) = self.find(hash, pull_mode, alg);
        if matched {
            let entry = &mut self.entries[index];
            debug_assert_ne!(entry.result ^ result, 1, "different cache value");
            if entry.result == SEARCH_EXCEEDED {
                entry.result = result;
            }
        } else {
            self.entries[index] = CacheEntry {
                hash,
                queries: 0,
                generation: self.generation,
                result,
                pull_mode: pull,
                alg: alg_id,
            };
            self.total += 1;
        }
    }

    pub fn queries(&self, hash: u64, pull_mode: bool, alg: SearchMode) -> u32 {
        let (index, matched) = self.find(hash, pull_mode, alg);
        if matched {
            self.entries[index].queries
        } else {
            0
        }
    }

    pub fn update_queries(&mut self, hash: u64, pull_mode: bool, alg: SearchMode) {
        let (index, matched) = self.find(hash, pull_mode, alg);
        if matched {
            self.entries[index].queries = self.entries[index].queries.saturating_add(1);
        }
    }

    pub fn is_deadlock(result: i8) -> bool {
        result == IS_DEADLOCK
    }

    pub fn is_not_deadlock(result: i8) -> bool {
        result == NOT_DEADLOCK
    }

    pub fn is_search_exceeded(result: i8) -> bool {
        result == SEARCH_EXCEEDED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_hides_entries_from_the_previous_generation() {
        let mut cache = DeadlockCache::new(10);
        cache.insert(42, IS_DEADLOCK, false, SearchMode::Normal);
        cache.update_queries(42, false, SearchMode::Normal);
        assert_eq!(cache.get(42, false, SearchMode::Normal), Some(IS_DEADLOCK));
        assert_eq!(cache.queries(42, false, SearchMode::Normal), 1);

        cache.clear();
        assert_eq!(cache.get(42, false, SearchMode::Normal), None);
        assert_eq!(cache.queries(42, false, SearchMode::Normal), 0);

        cache.insert(42, NOT_DEADLOCK, false, SearchMode::Normal);
        assert_eq!(cache.get(42, false, SearchMode::Normal), Some(NOT_DEADLOCK));
    }
}
