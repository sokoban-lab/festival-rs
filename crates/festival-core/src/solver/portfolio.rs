//! Forward plan portfolio for small levels.
//!
//! The packing search often finds several distinct parking orders. The first
//! one is not always the best target for the forward search, so each candidate
//! is probed for a few expansions and the most promising one is continued.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::constants::{MAX_SOL_LEN, SearchMode};
use crate::error::{Result, SolveError};
use crate::moves::Move;
use crate::planning::{ParkOrder, oop, park_order};
use crate::search::fess::{FessConfig, run_fess_into};
use crate::search::helper::Helper;
use crate::search::perimeter::Perimeter;
use crate::search::score::{ScoreElement, is_better_score};
use crate::search::tree::Tree;
use crate::solver::LevelContext;

/// Plans are probed equally; the first solver wins outright.
const MAX_PLANS: usize = 4;
const PROBE_EXPANSIONS: u64 = 32;

pub struct PortfolioRun {
    pub solved: bool,
    pub moves: Vec<Move>,
    pub failure: Option<SolveError>,
    pub expansions: u64,
}

/// Runs the portfolio when the level is small enough for probing to be cheap.
/// Returns `None` when the caller should run the ordinary forward search.
#[allow(clippy::too_many_arguments)]
pub fn run(
    ctx: &LevelContext,
    forward: SearchMode,
    backward: SearchMode,
    tree: &mut Tree,
    base_helper: &Helper,
    perimeter: &Perimeter,
    time_limit: Duration,
    max_expansions: Option<u64>,
    log_max_nodes: u32,
    save_best: bool,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<Option<PortfolioRun>> {
    if forward != SearchMode::ForwardWithBases
        || ctx.level.box_count > 12
        || ctx.level.index_num() > 80
    {
        return Ok(None);
    }

    let plans = collect_plans(ctx, backward, tree, base_helper);
    if plans.len() < 2 {
        return Ok(None);
    }

    let started = Instant::now();
    // Keep at least half of a fixed expansion budget for the chosen plan.
    let probe_limit = max_expansions.map_or(PROBE_EXPANSIONS, |limit| {
        (limit / (MAX_PLANS as u64 * 2)).clamp(1, PROBE_EXPANSIONS)
    });

    let mut probe_expansions = 0u64;
    let mut best: Option<(usize, ScoreElement, i32, Perimeter, Helper)> = None;
    for (index, plan) in plans.iter().enumerate() {
        let mut plan_helper = match prepare_plan(ctx, plan, backward) {
            Ok(helper) => helper,
            Err(_) => continue,
        };
        let probe_perimeter = perimeter.snapshot();
        let run = run_fess_into(
            ctx,
            &FessConfig {
                time_limit: Some(time_limit / 8),
                max_expansions: Some(probe_limit),
                log_max_nodes,
                save_best: false,
                cancel: cancel.clone(),
            },
            forward,
            tree,
            &mut plan_helper,
            &probe_perimeter,
        )?;
        probe_expansions += run.expansions;
        if run.solved {
            return Ok(Some(PortfolioRun {
                solved: true,
                moves: run.moves,
                failure: None,
                expansions: probe_expansions,
            }));
        }

        let Some(expansion) = tree.best_node_in_tree() else {
            continue;
        };
        let node = tree.expansions[expansion as usize].node;
        let score = tree.nodes[node as usize].score;
        let mut min_dist = i32::MAX;
        for data in &tree.expansions {
            let node = &tree.nodes[data.node as usize];
            if !node.deadlocked {
                min_dist = min_dist.min(node.score.dist_to_targets);
            }
        }
        let better = match &best {
            None => true,
            Some((_, best_score, best_min_dist, ..)) => {
                if score.dist_to_targets != best_score.dist_to_targets {
                    score.dist_to_targets < best_score.dist_to_targets
                } else if min_dist != *best_min_dist {
                    min_dist < *best_min_dist
                } else {
                    score.packed_boxes < best_score.packed_boxes
                }
            }
        };
        if better {
            best = Some((index, score, min_dist, probe_perimeter, plan_helper));
        }
    }

    let (probe_perimeter, mut plan_helper) = match best {
        Some((_, _, _, probe_perimeter, plan_helper)) => (probe_perimeter, plan_helper),
        None => match prepare_plan(ctx, &plans[0], backward) {
            Ok(plan_helper) => (perimeter.snapshot(), plan_helper),
            Err(_) => return Ok(None),
        },
    };
    let remaining_time = time_limit.saturating_sub(started.elapsed());
    let remaining_expansions =
        max_expansions.map(|limit| limit.saturating_sub(probe_expansions).max(1));
    let run = run_fess_into(
        ctx,
        &FessConfig {
            time_limit: Some(remaining_time),
            max_expansions: remaining_expansions,
            log_max_nodes,
            save_best,
            cancel,
        },
        forward,
        tree,
        &mut plan_helper,
        &probe_perimeter,
    )?;
    Ok(Some(PortfolioRun {
        solved: run.solved,
        moves: run.moves,
        failure: run.failure,
        expansions: probe_expansions + run.expansions,
    }))
}

fn prepare_plan(ctx: &LevelContext, plan: &[ParkOrder], backward: SearchMode) -> Result<Helper> {
    let mut helper = Helper {
        parking_order: plan.to_vec(),
        ..Helper::default()
    };
    park_order::verify_parking_order(ctx, &helper)?;
    park_order::reduce_parking_order(ctx, &mut helper)?;
    if backward != SearchMode::GirlSearch {
        oop::prepare_oop_zones(ctx, &mut helper)?;
    }
    helper.weighted_search = true;
    Ok(helper)
}

fn collect_plans(
    ctx: &LevelContext,
    backward: SearchMode,
    tree: &Tree,
    base_helper: &Helper,
) -> Vec<Vec<ParkOrder>> {
    let mut plans: Vec<Vec<ParkOrder>> = Vec::with_capacity(MAX_PLANS);
    let base = if base_helper.full_parking_order.is_empty() {
        base_helper.parking_order.clone()
    } else {
        base_helper.full_parking_order.clone()
    };
    if !base.is_empty() {
        plans.push(base);
    }

    let mut ids: Vec<u32> = (0..tree.expansions.len() as u32)
        .filter(|&id| {
            let expansion = &tree.expansions[id as usize];
            !tree.nodes[expansion.node as usize].deadlocked
                && expansion.depth as usize + ctx.level.box_count + 2 < MAX_SOL_LEN
        })
        .collect();
    ids.sort_by(|&a, &b| {
        let ea = &tree.expansions[a as usize];
        let eb = &tree.expansions[b as usize];
        let sa = &tree.nodes[ea.node as usize].score;
        let sb = &tree.nodes[eb.node as usize].score;
        if is_better_score(sa, sb, true, backward) {
            std::cmp::Ordering::Less
        } else if is_better_score(sb, sa, true, backward) {
            std::cmp::Ordering::Greater
        } else {
            a.cmp(&b)
        }
    });

    for id in ids.into_iter().take(512) {
        if plans.len() >= MAX_PLANS {
            break;
        }
        let mut candidate = Helper::default();
        if park_order::get_parking_plan(ctx, tree, id, &mut candidate).is_err() {
            continue;
        }
        if candidate.parking_order.is_empty() || plans.contains(&candidate.parking_order) {
            continue;
        }
        plans.push(candidate.parking_order);
    }
    plans
}
