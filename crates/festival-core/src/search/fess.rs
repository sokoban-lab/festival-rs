// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! FESS main loop (`engine.cpp`).
//!
//! Works for every Phase 4 forward mode; GIRL and Dragonfly stay for Phase 5.

use std::time::Duration;

use crate::constants::{MAX_TREE_LABELS, SearchMode, TARGET};
use crate::deadlock::cycle::check_cycle_deadlock;
use crate::deadlock::oracle::move_is_deadlocked;
use crate::error::{Result, SolveError};
use crate::moves::{Move, apply_move};
use crate::search::helper::Helper;
use crate::search::hf;
use crate::search::perimeter::Perimeter;
use crate::search::tree::{ExpansionId, NONE_EXPANSION, Tree};
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

/// Port of `has_winning_move` (`engine.cpp`).
///
/// In a cyclic level the solved position may already be in the tree (usually
/// as the root), so `node_is_solved` never sees it as a new descendant. This
/// detects the move that reaches such a node and re-roots the tree at a copy
/// with the targets cleared, which keeps the hash distinct.
fn has_winning_move(
    tree: &mut Tree,
    expansion: ExpansionId,
    ctx: &LevelContext,
    helper: &mut Helper,
) -> Result<bool> {
    if !ctx.level.is_cyclic() {
        return Ok(false);
    }

    let (start, count) = {
        let e = &tree.expansions[expansion as usize];
        (e.move_hash_place, e.moves_num as usize)
    };
    let boxes_on_targets = ctx.level.box_count as i16;

    let mut winning_son = None;
    for son in 0..count {
        let hash = tree.move_hashes[start + son].hash;
        let node = tree
            .find_node_by_hash(hash)
            .ok_or_else(|| SolveError::Internal("has_winning_move: missing node".into()))?;
        if tree.nodes[node as usize].score.boxes_on_targets == boxes_on_targets {
            winning_son = Some(son);
            break;
        }
    }

    let Some(son) = winning_son else {
        return Ok(false);
    };

    let mut board = tree.expansion_board(expansion)?;
    let mv = tree.move_hashes[start + son].move_data;
    apply_move(&mut board, &mv, SearchMode::Normal, &ctx.level);
    for y in 0..board.height() {
        for x in 0..board.width() {
            board.and_not(y, x, TARGET);
        }
    }

    let new_root = tree.set_root(ctx, board, helper)?;
    tree.set_advisors_and_weights(new_root, ctx, helper)?;

    tree.expansions[new_root as usize].father = expansion;
    tree.expansions[new_root as usize].depth = tree.expansions[expansion as usize].depth + 1;

    let new_hash = tree.nodes[tree.expansions[new_root as usize].node as usize].hash;
    tree.move_hashes[start + son].hash = new_hash;

    Ok(true)
}

/// Creates a private tree and runs one forward search. Used by tests and the
/// forward-only development path.
pub fn run_fess(
    ctx: &LevelContext,
    config: &FessConfig,
    search_mode: SearchMode,
) -> Result<FessRun> {
    let mut tree = Tree::new_with_level(config.log_max_nodes, &ctx.level)?;
    tree.prepare_deadlock(ctx)?;
    let mut helper = Helper::default();
    let mut perimeter = Perimeter::new(config.log_max_nodes.min(22));
    run_fess_into(
        ctx,
        config,
        search_mode,
        &mut tree,
        &mut helper,
        &mut perimeter,
    )
}

/// Runs a forward FESS search on an existing tree. The deadline in `config`
/// is relative to this call.
pub fn run_fess_into(
    ctx: &LevelContext,
    config: &FessConfig,
    search_mode: SearchMode,
    tree: &mut Tree,
    helper: &mut Helper,
    perimeter: &mut Perimeter,
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

    if search_mode == SearchMode::GirlSearch {
        return Err(SolveError::Internal(
            "GIRL_SEARCH is not implemented before Phase 5".into(),
        ));
    }

    let clock = SearchClock::new(config.time_limit, config.max_expansions);
    helper.init_scored_distance(ctx, &ctx.level.board, false);

    tree.reset();
    tree.pull_mode = false;
    tree.search_mode = search_mode;

    helper.begin_search();

    if perimeter.is_in(ctx.level.board.hash(), 1).is_some() {
        helper.enable_inefficient_moves = true;
    }

    let mut new_node = tree.set_root(ctx, ctx.level.board, helper)?;
    tree.set_advisors_and_weights(new_node, ctx, helper)?;
    tree.expansions[new_node as usize].best_past = tree.expansions[new_node as usize].node;

    let mut best_so_far = new_node;

    let root_score = tree.nodes[tree.expansions[new_node as usize].node as usize].score;
    helper.requests.clear();
    helper.requests.add_request(&root_score)?;
    let label = helper.requests.label_of_score(&root_score, false);
    tree.add_label_to_last_node(label, ctx, helper)?;

    perimeter.insert_node(ctx, tree, new_node, true);

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
        if move_is_deadlocked(
            ctx,
            &mut tree.deadlock,
            &board,
            &played_move,
            false,
            search_mode,
        )? {
            tree.set_node_as_deadlocked(ctx, expanded_node, tree_son as usize)?;
            continue;
        }

        expansion_count += 1;
        let child = tree.expand_node(ctx, expanded_node, tree_son as usize, helper)?;
        tree.set_advisors_and_weights(child, ctx, helper)?;
        new_node = child;

        let past = tree.expansions[new_node as usize].best_past;
        let past_score = tree.nodes[past as usize].score;
        helper.requests.add_request(&past_score)?;
        let label = helper.requests.label_of_score(&past_score, false);
        tree.add_label_to_last_node(label, ctx, helper)?;

        if tree.best_so_far_is_better(best_so_far, new_node) {
            best_so_far = new_node;
        }

        if tree.node_is_solved(new_node, ctx) {
            solved = true;
            break;
        }

        check_cycle_deadlock(tree, best_so_far, ctx);

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

        if has_winning_move(tree, new_node, ctx, helper)? {
            new_node = tree
                .last_expansion()
                .ok_or_else(|| SolveError::Internal("missing winning node".into()))?;
            solved = true;
            break;
        }

        tree.learn_from_subtrees(ctx, new_node)?;

        if tree.check_if_perimeter_reached(perimeter, ctx, helper) {
            let past = tree.expansions[new_node as usize].best_past;
            let score = tree.nodes[past as usize].score;
            helper.requests.set_next_request_to_score(&score);
        }

        if !helper.perimeter_found {
            perimeter.insert_node(ctx, tree, new_node, true);
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
            tree.store_solution(node, helper)?;
            moves = helper.sol_moves.clone();
        }
    }

    if search_mode == SearchMode::RevSearch {
        let board = tree.expansion_board(best_so_far)?;
        hf::setup_rev_board(ctx, &tree.deadlock, &board, helper)?;
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
    fn rewires_a_cyclic_winning_move() {
        let ctx = context("#######\n#@*   #\n#     #\n#######\n");
        assert!(ctx.level.is_cyclic());

        let mut tree = Tree::new_with_level(10, &ctx.level).unwrap();
        tree.reset();
        tree.pull_mode = false;
        tree.search_mode = SearchMode::Normal;
        let mut helper = Helper::default();
        helper.init_scored_distance(&ctx, &ctx.level.board, false);
        let root = tree.set_root(&ctx, ctx.level.board, &mut helper).unwrap();
        assert!(tree.expansions[root as usize].moves_num > 0);

        // A second position with every box on a target, distinct from the
        // root because the player stands elsewhere.
        let mut solved = ctx.level.board;
        solved.and_not(1, 1, crate::constants::SOKOBAN);
        solved.or(1, 3, crate::constants::SOKOBAN);
        let node = tree.add_board_to_tree(&ctx, &helper, &solved).unwrap();
        assert_eq!(
            tree.nodes[node as usize].score.boxes_on_targets,
            ctx.level.box_count as i16
        );

        let place = tree.expansions[root as usize].move_hash_place;
        tree.move_hashes[place].hash = solved.hash();

        assert!(has_winning_move(&mut tree, root, &ctx, &mut helper).unwrap());
        let last = tree.last_expansion().unwrap();
        assert_eq!(tree.expansions[last as usize].father, root);
        assert_eq!(tree.expansions[last as usize].depth, 1);
        assert_eq!(
            tree.move_hashes[place].hash,
            tree.nodes[tree.expansions[last as usize].node as usize].hash
        );
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
