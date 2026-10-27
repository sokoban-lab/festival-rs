// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! One top-level strategy: reverse search, plan setup, forward search
//! (`solve_with_alg` in `sokoban_solver.cpp`).
//!
//! Phase 4 implements strategies A/B/D/E/F/G (indices 0, 1, 3, 4, 5, 6).
//! Strategy C (GIRL) and H (Dragonfly) arrive in Phase 5/6.

use std::time::{Duration, Instant};

use crate::constants::SearchMode;
use crate::error::{Result, SolveError};
use crate::io::lurd;
use crate::io::solution::build_lurd;
use crate::moves::Move;
use crate::planning::{oop, park_order};
use crate::search::dragonfly::{DragonflyConfig, run_dragonfly};
use crate::search::fess::{FessConfig, run_fess_into};
use crate::search::helper::Helper;
use crate::search::packing::{PackingConfig, run_packing};
use crate::search::perimeter::Perimeter;
use crate::search::tree::Tree;
use crate::solver::LevelContext;
use crate::solver::forward::SolveOutcome;

#[derive(Clone, Debug)]
pub struct StrategyConfig {
    pub time_limit: Duration,
    pub max_expansions: Option<u64>,
    pub log_max_nodes: u32,
    pub save_best: bool,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            time_limit: Duration::from_secs(600),
            max_expansions: None,
            log_max_nodes: 20,
            save_best: false,
        }
    }
}

pub const BACKWARD_SEARCH_TYPES: [SearchMode; 8] = [
    SearchMode::BaseSearch,
    SearchMode::MaxDistSearch2,
    SearchMode::GirlSearch,
    SearchMode::HfSearch,
    SearchMode::BiconSearch,
    SearchMode::MaxDistSearch,
    SearchMode::RevSearch,
    SearchMode::Dragonfly,
];

pub const FORWARD_SEARCH_TYPES: [SearchMode; 8] = [
    SearchMode::ForwardWithBases,
    SearchMode::HfSearch,
    SearchMode::GirlSearch,
    SearchMode::HfSearch,
    SearchMode::HfSearch,
    SearchMode::HfSearch,
    SearchMode::RevSearch,
    SearchMode::NaiveSearch,
];

fn phase_limit(config: &StrategyConfig, duration: Duration) -> (Option<Duration>, Option<u64>) {
    if config.max_expansions.is_some() {
        (None, config.max_expansions)
    } else {
        (Some(duration), None)
    }
}

/// C++ computes sub-budgets in whole seconds (`time(0) %` arithmetic).
fn secs(duration: Duration) -> Duration {
    Duration::from_secs(duration.as_secs())
}

fn setup_plan_features(
    ctx: &LevelContext,
    helper: &mut Helper,
    backward: SearchMode,
) -> Result<Option<&'static str>> {
    if matches!(
        backward,
        SearchMode::HfSearch
            | SearchMode::BiconSearch
            | SearchMode::MaxDistSearch
            | SearchMode::MaxDistSearch2
            | SearchMode::RevSearch
            | SearchMode::NaiveSearch
            | SearchMode::Dragonfly
    ) {
        return Ok(None);
    }

    if helper.parking_order.is_empty() {
        return Ok(Some("Could not find packing order"));
    }

    park_order::verify_parking_order(ctx, helper)?;
    park_order::reduce_parking_order(ctx, helper)?;

    if backward == SearchMode::GirlSearch {
        return Ok(None);
    }

    oop::prepare_oop_zones(ctx, helper)?;
    Ok(None)
}

/// Runs one strategy end-to-end and produces the CLI-facing outcome.
pub fn solve_strategy(
    ctx: &LevelContext,
    strategy_index: usize,
    config: &StrategyConfig,
) -> Result<SolveOutcome> {
    if strategy_index >= 8 {
        return Err(SolveError::Internal(
            "algorithm number should be 0..7".into(),
        ));
    }
    let backward = BACKWARD_SEARCH_TYPES[strategy_index];
    let forward = FORWARD_SEARCH_TYPES[strategy_index];

    let started = Instant::now();
    let mut tree = Tree::new_with_level(config.log_max_nodes, &ctx.level)?;
    tree.prepare_deadlock(ctx)?;
    let mut helper = Helper::default();
    // Single-core C++ uses log2=25 for the shared frontier.
    let mut perimeter = Perimeter::new(25);

    let total = secs(config.time_limit);
    let backward_budget = Duration::from_secs(total.as_secs() / 3);

    // Reverse phase.
    helper.weighted_search = true;
    let mut total_expansions = 0u64;

    if backward == SearchMode::Dragonfly {
        // `packing_search_control` triples the 1/3 slot, so Dragonfly gets the
        // whole remaining budget. `--max-expansions` has no Dragonfly analogue.
        let dragonfly_budget = Duration::from_secs(backward_budget.as_secs().saturating_mul(3));
        let config = DragonflyConfig {
            time_limit: Some(dragonfly_budget),
            ..DragonflyConfig::default()
        };
        let run = run_dragonfly(
            ctx,
            &mut tree.deadlock,
            &mut helper,
            &mut perimeter,
            &config,
            &ctx.level.board,
        )?;
        total_expansions += run.expansions;
    } else if backward == SearchMode::RevSearch {
        let (time_limit, max_expansions) = phase_limit(config, backward_budget);
        let fess_config = FessConfig {
            time_limit,
            max_expansions,
            log_max_nodes: config.log_max_nodes,
            save_best: false,
        };
        let run = run_fess_into(
            ctx,
            &fess_config,
            backward,
            &mut tree,
            &mut helper,
            &mut perimeter,
        )?;
        total_expansions += run.expansions;
        if run.solved {
            return finish(
                ctx,
                run.solved,
                run.moves,
                run.failure,
                total_expansions,
                started,
            );
        }
    } else {
        let (time_limit, max_expansions) = phase_limit(config, backward_budget);
        let packing_config = PackingConfig {
            time_limit,
            max_expansions,
        };
        let run = run_packing(
            ctx,
            &mut tree,
            &mut helper,
            &mut perimeter,
            &packing_config,
            backward,
        )?;
        total_expansions += run.expansions;
    }

    if let Some(reason) = setup_plan_features(ctx, &mut helper, backward)? {
        return finish_unsolved(ctx, Some(reason), total_expansions, started);
    }

    // Forward phase.
    let elapsed = started.elapsed().as_secs();
    let remaining = Duration::from_secs(total.as_secs().saturating_sub(elapsed));
    helper.weighted_search = true;

    if forward == SearchMode::HfSearch {
        let first_budget = Duration::from_secs(remaining.as_secs() * 3 / 4);
        let (time_limit, max_expansions) = phase_limit(config, first_budget);
        helper.weighted_search = false;
        let first = run_fess_into(
            ctx,
            &FessConfig {
                time_limit,
                max_expansions,
                log_max_nodes: config.log_max_nodes,
                save_best: false,
            },
            forward,
            &mut tree,
            &mut helper,
            &mut perimeter,
        )?;
        total_expansions += first.expansions;
        if first.solved {
            return finish(
                ctx,
                true,
                first.moves,
                first.failure,
                total_expansions,
                started,
            );
        }

        let elapsed = started.elapsed().as_secs();
        let second_budget = Duration::from_secs(total.as_secs().saturating_sub(elapsed));
        let (time_limit, max_expansions) = phase_limit(config, second_budget);
        helper.weighted_search = true;
        let second = run_fess_into(
            ctx,
            &FessConfig {
                time_limit,
                max_expansions,
                log_max_nodes: config.log_max_nodes,
                save_best: config.save_best,
            },
            forward,
            &mut tree,
            &mut helper,
            &mut perimeter,
        )?;
        total_expansions += second.expansions;
        return finish(
            ctx,
            second.solved,
            second.moves,
            second.failure,
            total_expansions,
            started,
        );
    }

    if forward == SearchMode::RevSearch {
        // Reverse packing against the wallified forward position, then a
        // forward FESS that can only start if the perimeter was met.
        let (time_limit, max_expansions) = phase_limit(config, remaining);
        let packing = run_packing(
            ctx,
            &mut tree,
            &mut helper,
            &mut perimeter,
            &PackingConfig {
                time_limit,
                max_expansions,
            },
            forward,
        )?;
        total_expansions += packing.expansions;
        if !helper.perimeter_found {
            return finish_unsolved(
                ctx,
                packing
                    .failure
                    .as_ref()
                    .map(|failure| failure.reason())
                    .or(Some("Time limit exceeded")),
                total_expansions,
                started,
            );
        }

        let elapsed = started.elapsed().as_secs();
        let remaining = Duration::from_secs(total.as_secs().saturating_sub(elapsed));
        let (time_limit, max_expansions) = phase_limit(config, remaining);
        let run = run_fess_into(
            ctx,
            &FessConfig {
                time_limit,
                max_expansions,
                log_max_nodes: config.log_max_nodes,
                save_best: config.save_best,
            },
            forward,
            &mut tree,
            &mut helper,
            &mut perimeter,
        )?;
        total_expansions += run.expansions;
        return finish(
            ctx,
            run.solved,
            run.moves,
            run.failure,
            total_expansions,
            started,
        );
    }

    let (time_limit, max_expansions) = phase_limit(config, remaining);
    let run = run_fess_into(
        ctx,
        &FessConfig {
            time_limit,
            max_expansions,
            log_max_nodes: config.log_max_nodes,
            save_best: config.save_best,
        },
        forward,
        &mut tree,
        &mut helper,
        &mut perimeter,
    )?;
    total_expansions += run.expansions;
    finish(
        ctx,
        run.solved,
        run.moves,
        run.failure,
        total_expansions,
        started,
    )
}

fn finish(
    ctx: &LevelContext,
    solved: bool,
    moves: Vec<Move>,
    failure: Option<SolveError>,
    expansions: u64,
    started: Instant,
) -> Result<SolveOutcome> {
    finish_with_moves(
        ctx,
        solved,
        moves,
        failure.as_ref().map(|failure| failure.reason()),
        expansions,
        started,
    )
}

fn finish_unsolved(
    ctx: &LevelContext,
    failure: Option<&'static str>,
    expansions: u64,
    started: Instant,
) -> Result<SolveOutcome> {
    finish_with_moves(ctx, false, Vec::new(), failure, expansions, started)
}

fn finish_with_moves(
    ctx: &LevelContext,
    solved: bool,
    moves: Vec<Move>,
    failure: Option<&'static str>,
    expansions: u64,
    started: Instant,
) -> Result<SolveOutcome> {
    let text = build_lurd(&ctx.level, &moves)?;
    if solved && !text.solved {
        return Err(SolveError::Internal(
            "search reported a solved position but the LURD replay failed".into(),
        ));
    }
    if solved && !text.lurd.is_empty() {
        let stats = lurd::replay(&ctx.level.initial, &text.lurd)?;
        if !stats.solved {
            return Err(SolveError::Internal(
                "independent LURD validator rejected the solution".into(),
            ));
        }
    }

    Ok(SolveOutcome {
        solved,
        failure_reason: if solved {
            None
        } else {
            failure.or(Some("Unknown reason"))
        },
        lurd: text.lurd,
        moves: text.moves,
        pushes: text.pushes,
        expansions,
        elapsed: started.elapsed(),
    })
}

/// All eight C++ strategies are implemented.
pub fn strategy_supported(index: usize) -> bool {
    index < 8
}

/// Convenience for tests and the bench driver.
pub fn solve_level_strategy(
    ctx: &LevelContext,
    strategy_index: usize,
    time: Duration,
) -> Result<SolveOutcome> {
    solve_strategy(
        ctx,
        strategy_index,
        &StrategyConfig {
            time_limit: time,
            ..StrategyConfig::default()
        },
    )
}
