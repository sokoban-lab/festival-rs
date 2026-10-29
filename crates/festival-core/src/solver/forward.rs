//! Forward-only solve entry point used by the Phase 2 CLI.

use std::time::Duration;

use crate::constants::SearchMode;
use crate::error::{Result, SolveError};
use crate::io::lurd;
use crate::io::solution::build_lurd;
use crate::search::fess::{FessConfig, run_fess};
use crate::solver::LevelContext;

#[derive(Clone, Debug, Default)]
pub struct SolveOutcome {
    pub solved: bool,
    pub failure_reason: Option<&'static str>,
    pub lurd: String,
    pub moves: usize,
    pub pushes: usize,
    pub expansions: u64,
    pub elapsed: Duration,
}

pub fn solve_forward(ctx: &LevelContext, config: &FessConfig) -> Result<SolveOutcome> {
    let run = run_fess(ctx, config, SearchMode::Normal)?;
    let text = build_lurd(&ctx.level, &run.moves)?;

    if run.solved && !text.solved {
        return Err(SolveError::Internal(
            "search reported a solved position but the LURD replay failed".into(),
        ));
    }

    if !text.lurd.is_empty() {
        let stats = lurd::replay(&ctx.level.initial, &text.lurd)?;
        if run.solved && !stats.solved {
            return Err(SolveError::Internal(
                "independent LURD validator rejected the solution".into(),
            ));
        }
    }

    Ok(SolveOutcome {
        solved: run.solved,
        failure_reason: run.failure.as_ref().map(|failure| failure.reason()),
        lurd: text.lurd,
        moves: text.moves,
        pushes: text.pushes,
        expansions: run.expansions,
        elapsed: run.elapsed,
    })
}
