// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Immutable per-level analysis shared by all search threads.

use crate::analysis::{self, Hotspots, Rooms};
use crate::distance::{self, Distances};
use crate::error::Result;
use crate::level::Level;
use crate::search::girl::GirlParams;
use crate::search::snail::SnailParams;
use crate::solver::clock::SearchClock;

pub struct LevelContext {
    pub level: Level,
    pub distances: Distances,
    pub rooms: Rooms,
    pub hotspots: Hotspots,
    pub girl: GirlParams,
    pub snail: SnailParams,
}

impl LevelContext {
    /// Runs the Phase-1 static analysis in the same order as
    /// `preprocess_level` after the deterministic preprocessing step.
    pub fn analyse(level: Level, clock: &SearchClock) -> Result<Self> {
        let distances = distance::compute(&level);
        let rooms = analysis::rooms::analyse(&level, &level.board)?;
        let hotspots = analysis::hotspot::init(&level, &distances, clock.hotspot_abort_at());
        let girl = GirlParams::compute(&level);
        let snail = SnailParams::detect(&level);

        Ok(Self {
            level,
            distances,
            rooms,
            hotspots,
            girl,
            snail,
        })
    }
}
