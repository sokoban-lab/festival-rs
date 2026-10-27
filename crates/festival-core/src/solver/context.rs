// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Immutable per-level analysis shared by all search threads.

use crate::analysis::{self, EnvelopePatterns, Hotspots, Rooms};
use crate::constants::{INF, MAX_INNER};
use crate::distance::{self, Distances};
use crate::error::Result;
use crate::level::Level;
use crate::solver::clock::SearchClock;

pub struct LevelContext {
    pub level: Level,
    pub distances: Distances,
    pub rooms: Rooms,
    pub hotspots: Hotspots,
    pub envelopes: EnvelopePatterns,
    /// Push distance from each cell to the nearest target (`scored_distance`).
    pub scored_distance: [u32; MAX_INNER],
}

impl LevelContext {
    /// Runs the Phase-1 static analysis in the same order as
    /// `preprocess_level` after the deterministic preprocessing step.
    pub fn analyse(level: Level, clock: &SearchClock) -> Result<Self> {
        let distances = distance::compute(&level);
        let rooms = analysis::rooms::analyse(&level, &level.board)?;
        let hotspots = analysis::hotspot::init(&level, &distances, clock.hotspot_abort_at());

        let mut scored_distance = [INF; MAX_INNER];
        distance::get_distance_from_targets(&level, &level.board, &mut scored_distance);

        Ok(Self {
            level,
            distances,
            rooms,
            hotspots,
            envelopes: EnvelopePatterns::new(),
            scored_distance,
        })
    }
}
