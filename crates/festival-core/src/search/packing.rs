// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Reverse packing search (`packing_search.cpp`).
//!
//! The reverse search starts from the solved position (boxes on targets) and
//! pulls boxes back to their initial places. Its result is a parking order and
//! the imagined HF board used by the forward strategies.

use std::time::Duration;

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::cycle::check_cycle_deadlock;
use crate::deadlock::oracle::{can_played_reversed, move_is_deadlocked};
use crate::deadlock::stuck::tree_board_matches_patterns;
use crate::deadlock::wobblers::start_zone_is_deadlocked;
use crate::error::{Result, SolveError};
use crate::moves::find_possible_moves;
use crate::planning::{imagine, park_order};
use crate::search::helper::Helper;
use crate::search::perimeter::Perimeter;
use crate::search::tree::{ExpansionId, Tree};
use crate::solver::LevelContext;
use crate::solver::clock::SearchClock;

#[derive(Clone, Debug, Default)]
pub struct PackingConfig {
    pub time_limit: Option<Duration>,
    pub max_expansions: Option<u64>,
}

#[derive(Debug)]
pub struct PackingRun {
    pub plan_found: bool,
    pub expansions: u64,
    pub elapsed: Duration,
    pub failure: Option<SolveError>,
}

fn compute_start_zones(ctx: &LevelContext) -> ([u32; MAX_INNER], usize) {
    let mut b = ctx.level.initial;
    b.clear_boxes_inplace();
    b.turn_targets_into_walls();

    let mut zones = [INF; MAX_INNER];
    let num = crate::bfs::mark_connectivities(&ctx.level, &b, &mut zones);
    (zones, num)
}

fn put_sokoban_in_zone(board: &mut Board, zones: &[u32; MAX_INNER], z: u32) {
    board.clear_sokoban_inplace();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if zones[Board::flat_index(y, x)] == z && board.cell(y, x) & OCCUPIED == 0 {
                board.or(y, x, SOKOBAN);
            }
        }
    }
    board.expand_sokoban_cloud();
}

fn has_pull_moves(ctx: &LevelContext, tree: &mut Tree, board: &Board) -> Result<bool> {
    let mut helper = Helper::default();
    helper.init_scored_distance(ctx, board, true);
    let (moves, _) = find_possible_moves(
        ctx,
        board,
        true,
        SearchMode::Normal,
        &mut helper,
        &mut tree.deadlock,
    )?;
    Ok(!moves.is_empty())
}

fn declare_roots(
    ctx: &LevelContext,
    tree: &mut Tree,
    helper: &Helper,
    board: &Board,
    zones: &[u32; MAX_INNER],
    zones_num: usize,
) -> Result<()> {
    for z in 0..zones_num as u32 {
        let mut c = *board;
        put_sokoban_in_zone(&mut c, zones, z);
        if !has_pull_moves(ctx, tree, &c)? {
            continue;
        }
        tree.add_board_to_tree(ctx, helper, &c)?;
    }
    Ok(())
}

fn expansion_score(tree: &Tree, expansion: ExpansionId) -> crate::search::score::ScoreElement {
    let past = tree.expansions[expansion as usize].best_past;
    tree.nodes[past as usize].score
}

/// Port of `packing_search`.
pub fn run_packing(
    ctx: &LevelContext,
    tree: &mut Tree,
    helper: &mut Helper,
    perimeter: &mut Perimeter,
    config: &PackingConfig,
    search_mode: SearchMode,
) -> Result<PackingRun> {
    let clock = SearchClock::new(config.time_limit, config.max_expansions);
    if config.time_limit == Some(Duration::ZERO) {
        return Ok(PackingRun {
            plan_found: false,
            expansions: 0,
            elapsed: Duration::ZERO,
            failure: Some(SolveError::TimeLimit),
        });
    }

    helper.init_scored_distance(ctx, &ctx.level.board, true);

    tree.reset();
    tree.pull_mode = true;
    tree.search_mode = search_mode;

    helper.perimeter_found = false;
    helper.enable_inefficient_moves = false;

    let mut b = ctx.level.board;
    b.enter_reverse_mode(&ctx.level.initial, search_mode == SearchMode::BaseSearch);

    let (zones, zones_num) = compute_start_zones(ctx);
    helper.requests.clear();
    declare_roots(ctx, tree, helper, &b, &zones, zones_num)?;

    let mut best_so_far: Option<ExpansionId> = None;

    for z in 0..zones_num as u32 {
        let mut c = b;
        put_sokoban_in_zone(&mut c, &zones, z);

        if !has_pull_moves(ctx, tree, &c)? {
            continue;
        }
        if start_zone_is_deadlocked(ctx, &mut tree.deadlock, &c)? {
            continue;
        }

        let root = tree.set_root(ctx, c, helper)?;
        tree.set_advisors_and_weights(root, ctx, helper)?;

        let score = expansion_score(tree, root);
        helper.requests.add_pack_request(&score)?;
        let label = helper.requests.label_of_score(&score, true);
        tree.add_label_to_last_node(label, ctx, helper)?;

        if best_so_far.is_none() || tree.best_so_far_is_better(best_so_far.unwrap(), root) {
            best_so_far = Some(root);
        }
        perimeter.insert_node(ctx, tree, root, true);
    }

    let mut expansions = 0u64;
    let mut iteration = 0u64;
    let mut failed_cell = 0u32;
    let mut failure = None;

    loop {
        iteration += 1;

        if clock.should_stop(expansions) {
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
                .next_pack_request()
                .ok_or_else(|| SolveError::Internal("no pack requests".into()))?
        } else {
            let best = best_so_far.ok_or_else(|| SolveError::Internal("no packing best".into()))?;
            let score = expansion_score(tree, best);
            helper
                .requests
                .pack_request_of_score(&score)
                .ok_or_else(|| SolveError::Internal("missing pack request".into()))?
        };

        let label = helper.requests.label_of_pack_request(&request);
        let (tree_weight, tree_pos, tree_son) =
            tree.best_move_in_tree(usize::try_from(label).unwrap_or(usize::MAX), ctx);

        if helper.perimeter_found && tree_weight >= 0 {
            continue;
        }

        if failed_cell >= 10_000 {
            break;
        }
        if tree_pos < 0 {
            failed_cell += 1;
            continue;
        }
        failed_cell = 0;

        let expanded_node = tree_pos as u32;
        let played_move = tree.get_node_move(expanded_node, tree_son as usize);
        let board = tree.expansion_board(expanded_node)?;

        if move_is_deadlocked(
            ctx,
            &mut tree.deadlock,
            &board,
            &played_move,
            true,
            search_mode,
        )? {
            tree.set_node_as_deadlocked(ctx, expanded_node, tree_son as usize)?;
            continue;
        }

        if !can_played_reversed(ctx, &tree.deadlock, &board, &played_move, true, search_mode)? {
            tree.remove_son_from_tree(ctx, expanded_node, tree_son as usize);
            continue;
        }

        expansions += 1;
        let child = tree.expand_node(ctx, expanded_node, tree_son as usize, helper)?;
        tree.set_advisors_and_weights(child, ctx, helper)?;

        let score = expansion_score(tree, child);
        helper.requests.add_pack_request(&score)?;
        let label = helper.requests.label_of_score(&score, true);
        tree.add_label_to_last_node(label, ctx, helper)?;

        if let Some(best) = best_so_far {
            if tree.best_so_far_is_better(best, child) {
                best_so_far = Some(child);
            }
        } else {
            best_so_far = Some(child);
        }

        if tree.node_is_solved(child, ctx) {
            break;
        }

        if let Some(best) = best_so_far {
            check_cycle_deadlock(tree, best, ctx);

            if tree.nodes[tree.expansions[best as usize].node as usize].deadlocked {
                best_so_far = tree.best_node_in_tree();
            }
        }

        let Some(best) = best_so_far else {
            failure = Some(SolveError::Internal("packing tree is exhausted".into()));
            break;
        };

        let best_board = tree.expansion_board(best)?;
        if tree_board_matches_patterns(ctx, &tree.deadlock, &best_board, true) {
            tree.recheck_tree_with_patterns(ctx)?;
            best_so_far = tree.best_node_in_tree();
        }

        if tree.nodes[tree.expansions[child as usize].node as usize]
            .score
            .boxes_in_level
            == 0
        {
            break;
        }

        if tree.tree_nearly_full() {
            failure = Some(SolveError::ResourceExhausted);
            break;
        }

        let score = expansion_score(tree, child);
        if tree.check_if_perimeter_reached(perimeter, ctx, helper) {
            helper.requests.set_next_pack_request_to_score(&score);
        }

        if !helper.perimeter_found {
            perimeter.insert_node(ctx, tree, child, true);
        } else {
            perimeter.insert_node(ctx, tree, child, false);
        }
    }

    if let Some(best) = best_so_far {
        let board = tree.expansion_board(best)?;
        imagine::set_imagined_hf_board(&board, helper);
    }

    let plan_found = park_order::conclude_parking(ctx, tree, best_so_far, helper)?;
    let elapsed = clock.elapsed();

    Ok(PackingRun {
        plan_found,
        expansions,
        elapsed,
        failure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = SearchClock::new(Some(Duration::from_secs(60)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    #[test]
    fn base_search_produces_a_parking_order() {
        let ctx = context("########\n#@ $ . #\n#      #\n# $ .  #\n########\n");
        let mut tree = Tree::new_with_level(16, &ctx.level).unwrap();
        tree.prepare_deadlock(&ctx).unwrap();
        let mut helper = Helper::default();
        let mut perimeter = Perimeter::new(16);
        let config = PackingConfig {
            time_limit: None,
            max_expansions: Some(20_000),
        };

        let run = run_packing(
            &ctx,
            &mut tree,
            &mut helper,
            &mut perimeter,
            &config,
            SearchMode::BaseSearch,
        )
        .unwrap();

        assert!(
            run.plan_found,
            "failure: {:?}",
            run.failure.map(|e| e.reason())
        );
        assert!(!helper.parking_order.is_empty());
    }
}
