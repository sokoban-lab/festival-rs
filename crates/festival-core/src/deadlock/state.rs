// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Per-thread mutable state for the deadlock subsystem.
//!
//! The C++ solver keeps the deadlock cache, k-distance table, learned stuck
//! patterns and MPDB2 tables in process globals. Here they are explicit,
//! per-level state owned by the search thread.

use crate::analysis::EnvelopePatterns;
use crate::analysis::hungarian::HungarianCache;
use crate::board::Board;
use crate::deadlock::cache::DeadlockCache;
use crate::deadlock::k_dist::KDistHash;
use crate::deadlock::mpdb::Mpdb;
use crate::deadlock::rooms::init_rooms_deadlock;
use crate::deadlock::stuck::StuckPatterns;
use crate::solver::LevelContext;

/// C++ sizes the process-wide cache by core count; each Rust worker owns the
/// 1-core size, so eight workers match the C++ eight-core footprint.
const CACHE_LOG_SIZE: u32 = 22;

pub struct RoomsDeadlockData {
    pub room_with_corridors: Vec<Board>,
    pub room_size: Vec<usize>,
}

pub struct DeadlockState {
    pub cache: DeadlockCache,
    pub hungarian_cache: HungarianCache,
    pub mpdb: Mpdb,
    pub k_dist: KDistHash,
    pub stuck: StuckPatterns,
    pub rooms: RoomsDeadlockData,
    pub envelopes: EnvelopePatterns,
    /// C++ registers mini-corral patterns only at `verbose >= 4`.
    pub learn_mini_corral: bool,
    /// Disabling the cache must not change any verdict (Gate G3).
    pub cache_enabled: bool,
    /// Set while re-running a query whose cached verdict is hidden; keeps the
    /// recomputation from mutating the algorithmic state seen by later queries.
    pub suppress_cache_writes: bool,
}

impl Default for DeadlockState {
    fn default() -> Self {
        Self::new(20)
    }
}

impl DeadlockState {
    pub fn new(cache_log_size: u32) -> Self {
        Self {
            cache: DeadlockCache::new(cache_log_size),
            hungarian_cache: HungarianCache::new(),
            mpdb: Mpdb::default(),
            k_dist: KDistHash::new(crate::constants::MAX_SIZE, crate::constants::MAX_SIZE),
            stuck: StuckPatterns::default(),
            rooms: RoomsDeadlockData {
                room_with_corridors: Vec::new(),
                room_size: Vec::new(),
            },
            envelopes: EnvelopePatterns::new(),
            learn_mini_corral: false,
            cache_enabled: true,
            suppress_cache_writes: false,
        }
    }

    /// Cached verdict for marking sons / skipping repeated searches.
    ///
    /// With the cache disabled, final verdicts are recomputed instead of
    /// reused. `SEARCH_EXCEEDED` is not a memoized result but the algorithm's
    /// re-evaluation marker (C++ stores it and later revisits the hash), so it
    /// stays visible: hiding it would change which boards get re-searched and
    /// therefore the verdicts themselves.
    pub fn cache_get(
        &self,
        hash: u64,
        pull_mode: bool,
        alg: crate::constants::SearchMode,
    ) -> Option<i8> {
        let result = self.cache.get(hash, pull_mode, alg)?;
        if self.cache_enabled || result == crate::deadlock::SEARCH_EXCEEDED {
            Some(result)
        } else {
            None
        }
    }

    /// Unfiltered lookup for callers that own the cache-driven bookkeeping
    /// (mini-corral's 100-query gate).
    pub fn cache_peek(
        &self,
        hash: u64,
        pull_mode: bool,
        alg: crate::constants::SearchMode,
    ) -> Option<i8> {
        self.cache.get(hash, pull_mode, alg)
    }

    pub fn cache_insert(
        &mut self,
        hash: u64,
        result: i8,
        pull_mode: bool,
        alg: crate::constants::SearchMode,
    ) {
        if self.suppress_cache_writes {
            return;
        }
        self.cache.insert(hash, result, pull_mode, alg);
    }

    pub fn cache_queries(
        &self,
        hash: u64,
        pull_mode: bool,
        alg: crate::constants::SearchMode,
    ) -> u32 {
        self.cache.queries(hash, pull_mode, alg)
    }

    pub fn cache_update_queries(
        &mut self,
        hash: u64,
        pull_mode: bool,
        alg: crate::constants::SearchMode,
    ) {
        if self.suppress_cache_writes {
            return;
        }
        self.cache.update_queries(hash, pull_mode, alg);
    }

    /// True when the cache holds a final verdict that `cache_get` hides.
    pub fn cache_hit_is_hidden(
        &self,
        hash: u64,
        pull_mode: bool,
        alg: crate::constants::SearchMode,
    ) -> bool {
        !self.cache_enabled
            && matches!(
                self.cache_peek(hash, pull_mode, alg),
                Some(crate::deadlock::IS_DEADLOCK | crate::deadlock::NOT_DEADLOCK)
            )
    }

    /// Per-level initialization in `preprocess_level` order.
    pub fn prepare(ctx: &LevelContext) -> crate::error::Result<Self> {
        let mut state = Self::new(CACHE_LOG_SIZE);
        state.reprepare(ctx)?;
        Ok(state)
    }

    /// Rebuilds a state that still holds a previous level's data. The cache
    /// allocation is kept; `DeadlockCache::clear` only bumps its generation.
    pub fn reprepare(&mut self, ctx: &LevelContext) -> crate::error::Result<()> {
        self.clear();
        if self.cache.log_size() != CACHE_LOG_SIZE {
            self.cache = DeadlockCache::new(CACHE_LOG_SIZE);
        }
        self.learn_mini_corral = false;
        self.cache_enabled = true;
        self.suppress_cache_writes = false;
        self.rooms = init_rooms_deadlock(ctx);
        crate::deadlock::mpdb::build_mpdb2(ctx, self)?;
        crate::deadlock::mpdb::build_pull_mpdb2(ctx, self)?;
        crate::analysis::envelope::init_envelope_patterns(ctx, self)?;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.hungarian_cache = HungarianCache::new();
        self.k_dist.clear();
        self.stuck.clear();
        self.envelopes.clear();
    }
}
