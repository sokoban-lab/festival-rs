// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Core scheduler (`solve_with_time_control_*` in `sokoban_solver.cpp`).
//!
//! The C++ solver spreads the eight strategies over 1/2/4/8 worker threads.
//! Every worker owns its tree, helper and deadlock state; the perimeter is
//! shared through a mutex, and `any_core_solved` is an atomic cancellation
//! flag. A round is joined before the next one starts, so per-core result
//! order is stable.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::error::{Result, SolveError};
use crate::search::helper::Helper;
use crate::search::perimeter::Perimeter;
use crate::search::tree::Tree;
use crate::solver::LevelContext;
use crate::solver::forward::SolveOutcome;
use crate::solver::strategy::{StrategyConfig, solve_strategy_into};

/// Thread counts accepted by C++ `allocate_search_trees`.
pub const CORE_COUNTS: [usize; 4] = [1, 2, 4, 8];

/// C++ `get_number_of_cores` rounds the machine down to 1/2/4/8. The memory
/// thresholds are host policy; only the processor count is consulted here.
pub fn default_cores() -> usize {
    let available = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    match available {
        8.. => 8,
        4..=7 => 4,
        2..=3 => 2,
        _ => 1,
    }
}

#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub cores: usize,
    pub forced_alg: Option<usize>,
    pub time_limit: Duration,
    pub max_expansions: Option<u64>,
    pub log_max_nodes: u32,
    pub save_best: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            cores: 1,
            forced_alg: None,
            time_limit: Duration::from_secs(600),
            max_expansions: None,
            log_max_nodes: 20,
            save_best: false,
        }
    }
}

#[derive(Debug)]
pub struct LevelRun {
    pub solved: bool,
    pub failure_reason: Option<&'static str>,
    /// Final-round outcome per core, in core index order (C++ `helpers[]`).
    pub outcomes: Vec<Option<SolveOutcome>>,
    pub elapsed: Duration,
    pub expansions: u64,
}

impl LevelRun {
    /// Solutions written by `save_solution_if_found`: every non-empty helper
    /// with `-save_best`, otherwise the first solved core.
    pub fn solutions(&self, save_best: bool) -> Vec<&SolveOutcome> {
        if save_best {
            return self
                .outcomes
                .iter()
                .flatten()
                .filter(|outcome| !outcome.lurd.is_empty())
                .collect();
        }
        self.outcomes
            .iter()
            .flatten()
            .find(|outcome| outcome.solved)
            .into_iter()
            .collect()
    }

    /// Outcome recorded in `times.txt` (`level_sol_moves` comes from the last
    /// `save_sol_moves` call).
    pub fn reported(&self, save_best: bool) -> Option<&SolveOutcome> {
        if save_best {
            return self
                .outcomes
                .iter()
                .flatten()
                .rfind(|outcome| !outcome.lurd.is_empty());
        }
        self.outcomes
            .iter()
            .flatten()
            .find(|outcome| outcome.solved)
    }
}

fn remaining(started: Instant, config: &SchedulerConfig) -> Duration {
    config.time_limit.saturating_sub(started.elapsed())
}

/// C++ measures `time(0)` in whole seconds and stops only when
/// `elapsed_seconds > budget`, so a budget of N seconds runs for up to N + 1.
/// Budgets that truncate to zero still return zero (C++ skips the phase).
fn seconds_with_cpp_slack(secs: u64) -> Duration {
    if secs == 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(secs + 1)
    }
}

fn scaled(remaining: Duration, ratio: f64) -> Duration {
    seconds_with_cpp_slack((remaining.as_secs() as f64 * ratio) as u64)
}

fn full_phase(remaining: Duration) -> Duration {
    seconds_with_cpp_slack(remaining.as_secs())
}

fn strategy_config(
    config: &SchedulerConfig,
    time_limit: Duration,
    cancel: &Arc<AtomicBool>,
) -> StrategyConfig {
    StrategyConfig {
        time_limit,
        max_expansions: config.max_expansions,
        log_max_nodes: config.log_max_nodes,
        save_best: config.save_best,
        cancel: Some(Arc::clone(cancel)),
    }
}

fn run_worker(
    ctx: &LevelContext,
    alg: usize,
    config: &StrategyConfig,
    perimeter: &Perimeter,
    cancel: &Arc<AtomicBool>,
    worker: &mut WorkerState,
) -> Result<SolveOutcome> {
    // C++ `reset_helper` runs at the start of every strategy; the tree keeps
    // its deadlock cache and learned patterns, like `search_trees[core]`.
    worker.helper.reset();
    let outcome = solve_strategy_into(
        ctx,
        alg,
        config,
        &mut worker.tree,
        &mut worker.helper,
        perimeter,
    )?;
    if outcome.solved {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(outcome)
}

/// One core's persistent search state (C++ `search_trees[i]` / `helpers[i]`).
struct WorkerState {
    tree: Tree,
    helper: Helper,
}

fn worker_state(ctx: &LevelContext, config: &SchedulerConfig) -> Result<WorkerState> {
    let mut tree = Tree::new_with_level(config.log_max_nodes, &ctx.level)?;
    tree.prepare_deadlock(ctx)?;
    Ok(WorkerState {
        tree,
        helper: Helper::default(),
    })
}

/// Reusable solver state for a level set, mirroring C++ `search_trees[]` and
/// `helpers[]`: the tree arenas and deadlock caches are allocated once and
/// each level rebuilds only its level-specific tables.
#[derive(Default)]
pub struct Solver {
    workers: Vec<WorkerState>,
    log_max_nodes: u32,
}

impl Solver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prepares one tree and helper per core, reusing the previous level's.
    fn prepare_workers(&mut self, ctx: &LevelContext, config: &SchedulerConfig) -> Result<()> {
        if self.log_max_nodes != config.log_max_nodes {
            self.workers.clear();
            self.log_max_nodes = config.log_max_nodes;
        }
        let mut prepared = Vec::with_capacity(config.cores);
        for mut worker in std::mem::take(&mut self.workers)
            .into_iter()
            .take(config.cores)
        {
            if worker.tree.prepare_for_level(config.log_max_nodes, ctx)? {
                worker.helper.reset();
                prepared.push(worker);
            }
        }
        while prepared.len() < config.cores {
            prepared.push(worker_state(ctx, config)?);
        }
        self.workers = prepared;
        Ok(())
    }

    /// Runs a whole level with C++ scheduling semantics.
    pub fn solve_level(
        &mut self,
        ctx: &LevelContext,
        config: &SchedulerConfig,
    ) -> Result<LevelRun> {
        if !CORE_COUNTS.contains(&config.cores) {
            return Err(SolveError::Internal(
                "Number of cores should be 1/2/4/8".into(),
            ));
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let perimeter = Perimeter::new(25);
        let mut outcomes: Vec<Option<SolveOutcome>> = (0..config.cores).map(|_| None).collect();
        let mut expansions = 0u64;

        // Reuse the previous level's worker state when possible; C++ likewise
        // allocates one tree per core for the whole level set.
        self.prepare_workers(ctx, config)?;
        let workers = &mut self.workers;

        // C++ `start_time` is taken after preprocessing and tree set-up, so worker
        // preparation must not consume the search budget.
        let started = Instant::now();

        let mut run_round_into =
            |algs: &[usize], slots: &[usize], time_limit: Duration| -> Result<bool> {
                let results =
                    run_round(ctx, algs, config, time_limit, &perimeter, &cancel, workers);
                let mut solved = false;
                for (result, &slot) in results.into_iter().zip(slots) {
                    let outcome = result?;
                    expansions += outcome.expansions;
                    solved |= outcome.solved;
                    outcomes[slot] = Some(outcome);
                }
                Ok(solved)
            };

        let solved = if let Some(alg) = config.forced_alg {
            if alg >= 8 {
                return Err(SolveError::Internal(
                    "algorithm number should be 0..7".into(),
                ));
            }
            run_round_into(&[alg], &[0], full_phase(remaining(started, config)))?
        } else {
            match config.cores {
                1 => {
                    let mut solved = false;
                    for alg in 0..8 {
                        let budget = scaled(remaining(started, config), 1.0 / (8 - alg) as f64);
                        solved = run_round_into(&[alg], &[0], budget)?;
                        if solved {
                            break;
                        }
                    }
                    solved
                }
                2 => {
                    let mut solved = false;
                    for round in 0..4 {
                        let budget = scaled(remaining(started, config), 1.0 / (4 - round) as f64);
                        let algs = [round * 2, round * 2 + 1];
                        solved = run_round_into(&algs, &[0, 1], budget)?;
                        if solved {
                            break;
                        }
                    }
                    solved
                }
                4 => {
                    let mut solved = false;
                    for round in 0..2 {
                        let budget = scaled(remaining(started, config), 1.0 / (2 - round) as f64);
                        let algs = [round * 4, round * 4 + 1, round * 4 + 2, round * 4 + 3];
                        solved = run_round_into(&algs, &[0, 1, 2, 3], budget)?;
                        if solved {
                            break;
                        }
                    }
                    solved
                }
                8 => {
                    let budget = remaining(started, config);
                    let algs = [0, 1, 2, 3, 4, 5, 6, 7];
                    run_round_into(&algs, &[0, 1, 2, 3, 4, 5, 6, 7], budget)?
                }
                _ => unreachable!("core count validated above"),
            }
        };

        let failure_reason = if solved {
            None
        } else {
            outcomes
                .iter()
                .flatten()
                .filter_map(|outcome| outcome.failure_reason)
                .next_back()
                .or(Some("Unknown reason"))
        };

        Ok(LevelRun {
            solved,
            failure_reason,
            outcomes,
            elapsed: started.elapsed(),
            expansions,
        })
    }
}

/// Runs a whole level with a one-shot worker set.
pub fn solve_level(ctx: &LevelContext, config: &SchedulerConfig) -> Result<LevelRun> {
    Solver::new().solve_level(ctx, config)
}

/// Runs one round concurrently, returning outcomes in strategy order.
fn run_round(
    ctx: &LevelContext,
    algs: &[usize],
    config: &SchedulerConfig,
    time_limit: Duration,
    perimeter: &Perimeter,
    cancel: &Arc<AtomicBool>,
    workers: &mut [WorkerState],
) -> Vec<Result<SolveOutcome>> {
    if algs.len() == 1 {
        let worker = strategy_config(config, time_limit, cancel);
        return vec![run_worker(
            ctx,
            algs[0],
            &worker,
            perimeter,
            cancel,
            &mut workers[0],
        )];
    }

    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(algs.len());
        let mut worker_iter = workers.iter_mut();
        for &alg in algs {
            let worker = strategy_config(config, time_limit, cancel);
            let state = worker_iter.next().expect("one worker per strategy");
            handles
                .push(scope.spawn(move || run_worker(ctx, alg, &worker, perimeter, cancel, state)));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(SolveError::Internal("solver thread panicked".into())))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;
    use crate::solver::SearchClock;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    fn config(cores: usize) -> SchedulerConfig {
        SchedulerConfig {
            cores,
            max_expansions: Some(200_000),
            log_max_nodes: 16,
            ..SchedulerConfig::default()
        }
    }

    #[test]
    fn default_cores_is_a_valid_count() {
        assert!(CORE_COUNTS.contains(&default_cores()));
    }

    #[test]
    fn forced_algorithm_solves_a_small_level() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let run = solve_level(
            &ctx,
            &SchedulerConfig {
                forced_alg: Some(0),
                ..config(1)
            },
        )
        .unwrap();
        assert!(run.solved);
        assert_eq!(run.solutions(false).len(), 1);
    }

    #[test]
    fn single_core_schedule_tries_strategies_in_order() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let run = solve_level(&ctx, &config(1)).unwrap();
        assert!(run.solved);
        assert_eq!(run.outcomes.len(), 1);
        assert!(
            run.outcomes[0]
                .as_ref()
                .is_some_and(|outcome| outcome.solved)
        );
    }

    #[test]
    fn multi_core_runs_are_safe_and_replayable() {
        let ctx = context("########\n#@ $ . #\n#      #\n# $ .  #\n########\n");
        for cores in CORE_COUNTS {
            let run = solve_level(&ctx, &config(cores)).unwrap();
            assert!(run.solved, "cores {cores}");
            assert_eq!(run.outcomes.len(), cores);
            assert!(!run.solutions(false).is_empty(), "cores {cores}");
        }
    }

    #[test]
    fn reused_solver_matches_a_fresh_one() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let config = config(1);
        let fresh = solve_level(&ctx, &config).unwrap();
        let mut solver = Solver::new();
        let first = solver.solve_level(&ctx, &config).unwrap();
        let second = solver.solve_level(&ctx, &config).unwrap();
        for run in [&first, &second] {
            assert_eq!(run.solved, fresh.solved);
            assert_eq!(run.expansions, fresh.expansions);
            let moves = run.outcomes[0].as_ref().unwrap().moves;
            assert_eq!(moves, fresh.outcomes[0].as_ref().unwrap().moves);
        }
    }

    #[test]
    fn reused_solver_handles_geometry_changes() {
        let narrow = context("#######\n#@ $ .#\n#######\n");
        let wide = context("#########\n#@ $ .  #\n#       #\n#########\n");
        let config = config(1);
        let mut solver = Solver::new();
        let runs = [
            solver.solve_level(&narrow, &config).unwrap(),
            solver.solve_level(&wide, &config).unwrap(),
            solver.solve_level(&narrow, &config).unwrap(),
        ];
        for (level, run) in [&narrow, &wide, &narrow].into_iter().zip(runs) {
            let fresh = solve_level(level, &config).unwrap();
            assert_eq!(run.solved, fresh.solved);
            assert_eq!(run.expansions, fresh.expansions);
            assert_eq!(
                run.outcomes[0].as_ref().unwrap().moves,
                fresh.outcomes[0].as_ref().unwrap().moves
            );
        }
    }

    #[test]
    fn multicore_shared_state_smoke() {
        // Small enough to run under Miri's interpreter; exercises the shared
        // perimeter mutex and the cancellation atomic from two threads.
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let run = solve_level(
            &ctx,
            &SchedulerConfig {
                cores: 2,
                max_expansions: Some(5_000),
                log_max_nodes: 8,
                ..SchedulerConfig::default()
            },
        )
        .unwrap();
        assert!(run.solved);
        assert_eq!(run.outcomes.len(), 2);
    }

    #[test]
    fn invalid_core_count_is_rejected() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        assert!(solve_level(&ctx, &config(3)).is_err());
    }

    #[test]
    fn phase_budgets_match_cpp_integer_clock() {
        assert_eq!(
            scaled(Duration::from_secs(60), 1.0 / 8.0),
            Duration::from_secs(8)
        );
        assert_eq!(scaled(Duration::from_secs(1), 1.0 / 8.0), Duration::ZERO);
        assert_eq!(full_phase(Duration::from_secs(60)), Duration::from_secs(61));
    }
}
