//! Immutable per-level analysis shared by all search threads.

use std::sync::{Arc, OnceLock};

use crate::analysis::{self, EnvelopePatterns, Hotspots, Rooms};
use crate::deadlock::mpdb::Mpdb;
use crate::deadlock::state::{DeadlockState, RoomsDeadlockData};
use crate::distance::{self, Distances};
use crate::error::Result;
use crate::level::Level;
use crate::search::girl::GirlParams;
use crate::search::snail::SnailParams;
use crate::solver::clock::SearchClock;

#[derive(Clone)]
pub(crate) struct SharedDeadlockTables {
    pub rooms: Arc<RoomsDeadlockData>,
    pub mpdb: Arc<Mpdb>,
    pub envelopes: Arc<EnvelopePatterns>,
}

pub struct LevelContext {
    pub level: Level,
    pub distances: Distances,
    pub rooms: Rooms,
    pub hotspots: Hotspots,
    pub girl: GirlParams,
    pub snail: SnailParams,
    deadlock_tables: OnceLock<SharedDeadlockTables>,
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
            deadlock_tables: OnceLock::new(),
        })
    }

    pub(crate) fn populate_shared_deadlock_tables(&self, state: &mut DeadlockState) -> Result<()> {
        if let Some(tables) = self.deadlock_tables.get() {
            state.rooms = Arc::clone(&tables.rooms);
            state.mpdb = Arc::clone(&tables.mpdb);
            state.envelopes = Arc::clone(&tables.envelopes);
            return Ok(());
        }

        state.rooms = Arc::new(crate::deadlock::rooms::init_rooms_deadlock(self));
        crate::deadlock::mpdb::build_mpdb2(self, state)?;
        crate::deadlock::mpdb::build_pull_mpdb2(self, state)?;
        crate::analysis::envelope::init_envelope_patterns(self, state)?;

        let _ = self.deadlock_tables.set(SharedDeadlockTables {
            rooms: Arc::clone(&state.rooms),
            mpdb: Arc::clone(&state.mpdb),
            envelopes: Arc::clone(&state.envelopes),
        });
        Ok(())
    }
}
