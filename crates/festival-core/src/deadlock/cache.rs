//! Local and cross-worker deadlock result caches (`deadlock_cache.cpp`).

use std::sync::atomic::{AtomicU64, Ordering};

use crate::constants::SearchMode;

const NOT_DEADLOCK: i8 = 0;
const IS_DEADLOCK: i8 = 1;
const SEARCH_EXCEEDED: i8 = 2;

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct CacheEntry {
    hash: u64,
    queries: u16,
    /// Bumped on `clear`; entries from an older generation read as empty, so a
    /// per-level reset only clears the 4M-entry table once every 65,535 levels.
    generation: u16,
    result: i8,
    pull_mode: i8,
    alg: i8,
    _pad: u8,
}

const _: () = assert!(std::mem::size_of::<CacheEntry>() == 16);

pub struct DeadlockCache {
    entries: Vec<CacheEntry>,
    mask: u64,
    total: usize,
    log_size: u32,
    generation: u16,
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

    #[inline]
    fn find(&self, hash: u64, pull_mode: bool, alg: SearchMode) -> (usize, bool) {
        let mut index = (hash & self.mask) as usize;
        let pull = pull_mode as i8;
        let alg = alg as i8;
        let cur_gen = self.generation;
        loop {
            let entry = &self.entries[index];
            if entry.hash == 0 || entry.generation != cur_gen {
                return (index, false);
            }
            if entry.hash == hash && entry.pull_mode == pull && entry.alg == alg {
                return (index, true);
            }
            index = (index + 1) & self.mask as usize;
        }
    }

    #[inline]
    pub fn get(&self, hash: u64, pull_mode: bool, alg: SearchMode) -> Option<i8> {
        let (index, matched) = self.find(hash, pull_mode, alg);
        if matched {
            Some(self.entries[index].result)
        } else {
            None
        }
    }

    #[inline]
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
                _pad: 0,
            };
            self.total += 1;
        }
    }

    #[inline]
    pub fn queries(&self, hash: u64, pull_mode: bool, alg: SearchMode) -> u32 {
        let (index, matched) = self.find(hash, pull_mode, alg);
        if matched {
            self.entries[index].queries as u32
        } else {
            0
        }
    }

    #[inline]
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

/// Lock-free 2-way set-associative cache of final (`IS_DEADLOCK` / `NOT_DEADLOCK`)
/// verdicts shared across multi-core workers on the same level.
///
/// With `2^19` sets (`2^20` `AtomicU64` slots = 8 MiB), the set index encodes
/// the low 19 bits of `hash` and bits `19..64` of the atomic word store the
/// remaining 45 bits of `hash`, reconstructing all 64 hash bits losslessly
/// without collisions:
/// - bits `19..64` (45b): `hash >> 19`
/// - bits `8..19`  (11b): `generation` (`1..=2047`, `0` = empty)
/// - bits `3..8`   (5b):  `alg` (`SearchMode` `0..=20`)
/// - bit  `2`      (1b):  `pull_mode`
/// - bits `0..2`   (2b):  verdict (`1 = NOT_DEADLOCK`, `2 = IS_DEADLOCK`)
pub struct SharedDeadlockCache {
    slots: Box<[AtomicU64]>,
    generation: u16,
}

const SHARED_SET_BITS: u32 = 19;
const SHARED_SETS: usize = 1usize << SHARED_SET_BITS;
const SHARED_SET_MASK: usize = SHARED_SETS - 1;
const SHARED_WAYS: usize = 2;
const SHARED_MAX_GEN: u16 = (1u16 << 11) - 1;

impl Default for SharedDeadlockCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedDeadlockCache {
    pub fn new() -> Self {
        let len = SHARED_SETS * SHARED_WAYS;
        let mut vec = Vec::with_capacity(len);
        vec.resize_with(len, || AtomicU64::new(0));
        Self {
            slots: vec.into_boxed_slice(),
            generation: 1,
        }
    }

    /// Advances the generation for a new level before workers are spawned.
    pub fn reset_for_new_level(&mut self) {
        if self.generation >= SHARED_MAX_GEN {
            for slot in self.slots.iter_mut() {
                *slot.get_mut() = 0;
            }
            self.generation = 1;
        } else {
            self.generation += 1;
        }
    }

    #[inline]
    fn key_prefix(hash: u64, generation: u16, pull_mode: bool, alg: SearchMode) -> u64 {
        (hash & !((1u64 << SHARED_SET_BITS) - 1))
            | ((generation as u64) << 8)
            | (((alg as u8 as u64) & 0x1f) << 3)
            | ((u64::from(pull_mode)) << 2)
    }

    #[inline]
    pub fn get(&self, hash: u64, pull_mode: bool, alg: SearchMode) -> Option<i8> {
        let base = ((hash as usize) & SHARED_SET_MASK) * SHARED_WAYS;
        let expected = Self::key_prefix(hash, self.generation, pull_mode, alg);

        for way in 0..SHARED_WAYS {
            let word = self.slots[base + way].load(Ordering::Relaxed);
            if (word & !0x3) == expected {
                return match word & 0x3 {
                    1 => Some(NOT_DEADLOCK),
                    2 => Some(IS_DEADLOCK),
                    _ => None,
                };
            }
        }
        None
    }

    #[inline]
    pub fn insert(&self, hash: u64, result: i8, pull_mode: bool, alg: SearchMode) {
        let code = match result {
            NOT_DEADLOCK => 1u64,
            IS_DEADLOCK => 2u64,
            _ => return,
        };
        let base = ((hash as usize) & SHARED_SET_MASK) * SHARED_WAYS;
        let cur_gen = self.generation as u64;
        let prefix = Self::key_prefix(hash, self.generation, pull_mode, alg);
        let word = prefix | code;

        let w0 = self.slots[base].load(Ordering::Relaxed);
        if (w0 & !0x3) == prefix || ((w0 >> 8) & 0x7ff) != cur_gen {
            self.slots[base].store(word, Ordering::Relaxed);
            return;
        }
        let w1 = self.slots[base + 1].load(Ordering::Relaxed);
        if (w1 & !0x3) == prefix || ((w1 >> 8) & 0x7ff) != cur_gen {
            self.slots[base + 1].store(word, Ordering::Relaxed);
            return;
        }
        let victim = ((hash >> SHARED_SET_BITS) as usize) & 1;
        self.slots[base + victim].store(word, Ordering::Relaxed);
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

    #[test]
    fn shared_cache_reconstructs_full_64bit_keys_and_generations() {
        let mut shared = SharedDeadlockCache::new();
        let h1 = 0xdead_beef_cafe_1234u64;
        // Same low 19 bits as h1, different upper 45 bits -> occupies way 1.
        let h2 = h1 ^ (1u64 << 25);
        // Differs by 1 bit in upper hash -> must NOT false-match h1 or h2.
        let h3 = h1 ^ (1u64 << 63);

        shared.insert(h1, IS_DEADLOCK, false, SearchMode::DeadlockSearch);
        shared.insert(h2, NOT_DEADLOCK, true, SearchMode::MiniCorral);
        shared.insert(h3, SEARCH_EXCEEDED, false, SearchMode::DeadlockSearch);

        assert_eq!(
            shared.get(h1, false, SearchMode::DeadlockSearch),
            Some(IS_DEADLOCK)
        );
        assert_eq!(shared.get(h1, true, SearchMode::DeadlockSearch), None);
        assert_eq!(shared.get(h1, false, SearchMode::MiniCorral), None);
        assert_eq!(
            shared.get(h2, true, SearchMode::MiniCorral),
            Some(NOT_DEADLOCK)
        );
        assert_eq!(shared.get(h3, false, SearchMode::DeadlockSearch), None);

        shared.reset_for_new_level();
        assert_eq!(shared.get(h1, false, SearchMode::DeadlockSearch), None);
        assert_eq!(shared.get(h2, true, SearchMode::MiniCorral), None);
    }
}
