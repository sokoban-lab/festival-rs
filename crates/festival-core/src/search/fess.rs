// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! FESS main loop (`engine.cpp`).
//!
//! Phase 2 runs the forward search only. Mode-specific hooks (GIRL, cycle
//! deadlock, learned patterns, REV) are intentionally absent rather than
//! silently approximated.

use std::time::Duration;

use crate::constants::{MAX_TREE_LABELS, SearchMode};
use crate::deadlock::oracle;
use crate::error::{Result, SolveError};
use crate::moves::Move;
use crate::search::helper::Helper;
use crate::search::perimeter::Perimeter;
use crate::search::tree::{NONE_EXPANSION, Tree};
use crate::solver::LevelContext;
use crate::solver::clock::SearchClock;

const MAX_WEIGHT: i32 = 1_000_000;

#[derive(Clone, Debug)]
pub struct FessConfig {
    pub time_limit: Option<Duration>,
    pub max_expansions: Option<u64>,
    pub log_max_nodes: u32,
    pub save_best: bool,
}

impl Default for FessConfig {
    fn default() -> Self {
        Self {
            time_limit: None,
            max_expansions: None,
            log_max_nodes: 20,
            save_best: false,
        }
    }
}

#[derive(Debug)]
pub struct FessRun {
    pub solved: bool,
    pub moves: Vec<Move>,
    pub failure: Option<SolveError>,
    pub expansions: u64,
    pub elapsed: Duration,
}

pub fn run_fess(
    ctx: &LevelContext,
    config: &FessConfig,
    search_mode: SearchMode,
) -> Result<FessRun> {
    if config.time_limit == Some(Duration::ZERO) {
        return Ok(FessRun {
            solved: false,
            moves: Vec::new(),
            failure: Some(SolveError::TimeLimit),
            expansions: 0,
            elapsed: Duration::ZERO,
        });
    }

    let clock = SearchClock::new(config.time_limit, config.max_expansions);
    let mut tree = Tree::new_with_level(config.log_max_nodes, &ctx.level)?;
    let mut perimeter = Perimeter::new(config.log_max_nodes.min(22));
    let mut helper = Helper::default();

    tree.reset();
    tree.pull_mode = false;
    tree.search_mode = search_mode;
    helper.reset();

    if perimeter.is_in(ctx.level.board.hash(), 1).is_some() {
        helper.enable_inefficient_moves = true;
    }

    let mut new_node = tree.set_root(ctx, ctx.level.board, &mut helper)?;
    tree.set_advisors_and_weights(new_node, ctx, &mut helper);
    tree.expansions[new_node as usize].best_past = tree.expansions[new_node as usize].node;

    let mut best_so_far = new_node;

    let root_score = tree.nodes[tree.expansions[new_node as usize].node as usize].score;
    helper.requests.clear();
    helper.requests.add_request(&root_score)?;
    let label = helper.requests.label_of_score(&root_score, false);
    tree.add_label_to_last_node(label, ctx, &mut helper)?;

    perimeter.insert_node(&tree, new_node, true);

    let mut solved = false;
    let mut expansion_count = 0u64;
    let mut iteration = 0u64;
    let mut failure = None;

    loop {
        iteration += 1;

        if clock.should_stop(expansion_count) {
            failure = Some(if clock.expired() {
                SolveError::TimeLimit
            } else {
                SolveError::ResourceExhausted
            });
            break;
        }

        let request = if (iteration & 1) == 1 || helper.perimeter_found {
            helper
                .requests
                .next_request()
                .ok_or_else(|| SolveError::Internal("no requests".into()))?
        } else {
            let past = tree.expansions[best_so_far as usize].best_past;
            let score = tree.nodes[past as usize].score;
            helper
                .requests
                .request_of_score(&score)
                .ok_or_else(|| SolveError::Internal("missing request".into()))?
        };

        if search_mode == SearchMode::GirlSearch && !helper.perimeter_found {
            return Err(SolveError::Internal(
                "GIRL_SEARCH is not implemented in Phase 2".into(),
            ));
        }

        let label = helper.requests.label_of_request(&request);
        if !(0..MAX_TREE_LABELS as i32).contains(&label) {
            return Err(SolveError::Internal("bad request label".into()));
        }

        let (tree_weight, tree_pos, tree_son) = tree.best_move_in_tree(label as usize, ctx);

        if helper.perimeter_found && tree_weight >= 0 {
            continue;
        }
        if tree_weight == MAX_WEIGHT {
            if tree.all_queues_empty() {
                failure = Some(SolveError::Internal("tree is exhausted".into()));
                break;
            }
            continue;
        }
        if tree_pos < 0 || tree_son < 0 {
            continue;
        }

        let expanded_node = tree_pos as u32;
        let played_move = tree.get_node_move(expanded_node, tree_son as usize);

        let board = tree.expansion_board(expanded_node)?;
        if oracle::move_is_deadlocked(&ctx.level, &board, &played_move, false) {
            tree.set_node_as_deadlocked(ctx, expanded_node, tree_son as usize)?;
            continue;
        }

        expansion_count += 1;
        let child = tree.expand_node(ctx, expanded_node, tree_son as usize, &mut helper)?;
        tree.set_advisors_and_weights(child, ctx, &mut helper);
        new_node = child;

        let past = tree.expansions[new_node as usize].best_past;
        let past_score = tree.nodes[past as usize].score;
        helper.requests.add_request(&past_score)?;
        let label = helper.requests.label_of_score(&past_score, false);
        tree.add_label_to_last_node(label, ctx, &mut helper)?;

        if tree.best_so_far_is_better(best_so_far, new_node) {
            best_so_far = new_node;
        }

        if tree.node_is_solved(new_node, ctx) {
            solved = true;
            break;
        }

        if tree.nodes[tree.expansions[best_so_far as usize].node as usize].deadlocked {
            match tree.best_node_in_tree() {
                Some(next) => best_so_far = next,
                None => {
                    failure = Some(SolveError::Internal("tree is exhausted".into()));
                    break;
                }
            }
        }

        if tree.tree_nearly_full() {
            failure = Some(SolveError::ResourceExhausted);
            break;
        }

        if tree.check_if_perimeter_reached(&perimeter, ctx, &mut helper) {
            let past = tree.expansions[new_node as usize].best_past;
            let score = tree.nodes[past as usize].score;
            helper.requests.set_next_request_to_score(&score);
        }

        if !helper.perimeter_found {
            perimeter.insert_node(&tree, new_node, true);
        }
    }

    helper.level_solved = solved;

    let mut moves = Vec::new();
    let mut solved_node = None;
    if solved {
        solved_node = Some(new_node);
    } else if config.save_best {
        solved_node = Some(best_so_far);
    }

    if let Some(node) = solved_node {
        if node != NONE_EXPANSION {
            tree.store_solution(node, &mut helper)?;
            moves = helper.sol_moves.clone();
        }
    }

    let elapsed = clock.elapsed();
    if !solved && failure.is_none() {
        failure = Some(if clock.expired() {
            SolveError::TimeLimit
        } else {
            SolveError::Internal("search stopped".into())
        });
    }

    Ok(FessRun {
        solved,
        moves,
        failure,
        expansions: expansion_count,
        elapsed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    #[test]
    fn solves_a_one_push_level() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let config = FessConfig {
            max_expansions: Some(10_000),
            ..FessConfig::default()
        };
        let run = run_fess(&ctx, &config, SearchMode::Normal).unwrap();
        assert!(
            run.solved,
            "failure: {:?}",
            run.failure.map(|err| err.reason())
        );
        assert_eq!(run.moves.len(), 1);
    }

    #[test]
    fn solves_a_two_box_level() {
        let ctx = context("########\n#@ $ . #\n#      #\n# $ .  #\n########\n");
        let config = FessConfig {
            max_expansions: Some(100_000),
            ..FessConfig::default()
        };
        let run = run_fess(&ctx, &config, SearchMode::Normal).unwrap();
        assert!(
            run.solved,
            "failure: {:?}",
            run.failure.map(|err| err.reason())
        );
        assert_eq!(run.moves.len(), 2);
    }
}
