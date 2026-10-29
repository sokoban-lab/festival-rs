//! Corral deadlock search (`corral_deadlock.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::mini::can_put_all_on_targets;
use crate::deadlock::positions::PositionSet;
use crate::deadlock::state::DeadlockState;
use crate::deadlock::wobblers::remove_wobblers;
use crate::deadlock::zone::{
    clear_boxes_out_of_deadlock_zone, clear_frozen_boxes_away_from_deadlock_zone,
    exited_deadlock_zone, init_deadlock_zone_from_group, set_deadlock_zone_in_pull_mode,
    work_left_in_deadlock_zone, work_left_outside_deadlock_zone,
};
use crate::deadlock::{IS_DEADLOCK, NOT_DEADLOCK, SEARCH_EXCEEDED};
use crate::error::Result;
use crate::moves::{Move, apply_move};
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

const MAX_DEADLOCK_SEARCH: usize = 50;

/// `check_for_deadlock_zone_moves`: a move leaving the zone becomes a kill.
pub fn check_for_deadlock_zone_moves_at(
    ctx: &LevelContext,
    board: &Board,
    moves: &mut [Move],
    moves_num: usize,
) -> usize {
    for i in 0..moves_num {
        let (y, x) = ctx.level.coord(moves[i].to as usize);
        if board.cell(y, x) & DEADLOCK_ZONE != 0 {
            continue;
        }
        moves[0] = moves[i];
        moves[0].kill = true;
        return 1;
    }
    moves_num
}

pub fn board_contains_more_deadlocks(new_board: &Board, old_board: &Board) -> bool {
    for y in 0..new_board.height() {
        for x in 0..new_board.width() {
            if new_board.cell(y, x) & DEADLOCK_ZONE != 0
                && old_board.cell(y, x) & DEADLOCK_ZONE == 0
            {
                return true;
            }
        }
    }
    false
}

pub fn is_better_deadlock_score(new_score: &ScoreElement, old_score: &ScoreElement) -> bool {
    if new_score.connectivity < old_score.connectivity {
        return true;
    }
    if new_score.connectivity > old_score.connectivity {
        return false;
    }
    if new_score.boxes_in_level < old_score.boxes_in_level {
        return true;
    }
    if new_score.boxes_in_level > old_score.boxes_in_level {
        return false;
    }
    if new_score.hotspots < old_score.hotspots {
        return true;
    }
    if new_score.hotspots > old_score.hotspots {
        return false;
    }
    false
}

fn choose_deadlock_son(positions: &PositionSet) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut best_score = ScoreElement::default();
    for i in 0..positions.len() {
        if positions.positions[i].position_deadlocked {
            continue;
        }
        for j in 0..positions.positions[i].moves.len() {
            if positions.positions[i].move_deadlocked[j]
                || positions.positions[i].place_in_positions[j].is_some()
            {
                continue;
            }
            if best.is_none()
                || is_better_deadlock_score(&positions.positions[i].scores[j], &best_score)
            {
                best = Some((i, j));
                best_score = positions.positions[i].scores[j];
            }
        }
    }
    best
}

fn son_can_break_free(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    positions: &PositionSet,
    p: usize,
) -> bool {
    let position = &positions.positions[p];
    if !position.pull_mode && !work_left_in_deadlock_zone(&position.board) {
        return true;
    }

    for i in 0..position.moves.len() {
        if position.move_deadlocked[i] {
            continue;
        }
        if position.scores[i].hotspots == 0 || position.scores[i].connectivity == 1 {
            return true;
        }
        // Learned non-deadlock knowledge is algorithmic state: the search uses
        // it for sound early exits, so it must stay visible with the memo off.
        if state.cache_peek(
            position.hashes[i],
            position.pull_mode,
            SearchMode::DeadlockSearch,
        ) == Some(NOT_DEADLOCK)
        {
            return true;
        }
        if position.pull_mode {
            let mut c = position.board;
            apply_move(
                &mut c,
                &position.moves[i],
                SearchMode::DeadlockSearch,
                &ctx.level,
            );
            if board_contains_more_deadlocks(&c, &position.board) || exited_deadlock_zone(&c) {
                return true;
            }
        }
    }
    false
}

fn mark_deadlocked_sons_using_cache(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    positions: &mut PositionSet,
) {
    let node = positions.len() - 1;
    let (pull_mode, hashes, places) = {
        let p = &positions.positions[node];
        (p.pull_mode, p.hashes.clone(), p.place_in_positions.clone())
    };
    for (i, hash) in hashes.iter().enumerate() {
        if state.cache_peek(*hash, pull_mode, SearchMode::DeadlockSearch) == Some(IS_DEADLOCK)
            && places[i].is_none()
        {
            positions.set_leaf_as_deadlock(ctx, node, i);
        }
    }
}

fn insert_positions_to_deadlock_cache(
    state: &mut DeadlockState,
    positions: &PositionSet,
    result: i8,
) {
    for position in &positions.positions {
        if position.position_deadlocked || result == IS_DEADLOCK {
            state.cache_insert(
                position.position_hash,
                IS_DEADLOCK,
                position.pull_mode,
                SearchMode::DeadlockSearch,
            );
        }
    }

    if result == NOT_DEADLOCK && !positions.positions.is_empty() {
        let mut index = Some(positions.len() - 1);
        while let Some(i) = index {
            let p = &positions.positions[i];
            state.cache_insert(
                p.position_hash,
                NOT_DEADLOCK,
                p.pull_mode,
                SearchMode::DeadlockSearch,
            );
            index = p.father;
        }
    }
}

pub fn corral_deadlock_search_actual(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
    search_size: usize,
) -> Result<i8> {
    let mut helper = Helper::default();

    if !pull_mode && !work_left_in_deadlock_zone(board) {
        return Ok(NOT_DEADLOCK);
    }

    let mut positions = PositionSet::new(search_size);
    positions.set_first_position(
        ctx,
        board,
        pull_mode,
        SearchMode::DeadlockSearch,
        &mut helper,
        state,
    )?;

    let result = loop {
        if positions.estimate_uniqe_moves() > search_size {
            break SEARCH_EXCEEDED;
        }

        let last = positions.len() - 1;
        if son_can_break_free(ctx, state, &positions, last) {
            break NOT_DEADLOCK;
        }

        mark_deadlocked_sons_using_cache(ctx, state, &mut positions);

        let Some((node, son)) = choose_deadlock_son(&positions) else {
            break IS_DEADLOCK;
        };

        positions.expand_position(ctx, node, son, pull_mode, &mut helper, state)?;

        if positions.len() >= search_size {
            break SEARCH_EXCEEDED;
        }
    };

    insert_positions_to_deadlock_cache(state, &positions, result);
    Ok(result)
}

pub fn mini_corral(ctx: &LevelContext, state: &mut DeadlockState, board: &Board) -> Result<i8> {
    if board.boxes_in_level() > 5 {
        return Ok(NOT_DEADLOCK);
    }
    let hash = board.hash();
    let hidden = state.cache_hit_is_hidden(hash, false, SearchMode::MiniCorral);
    match state.cache_peek(hash, false, SearchMode::MiniCorral) {
        Some(cached @ (IS_DEADLOCK | NOT_DEADLOCK)) if state.cache_enabled => return Ok(cached),
        Some(IS_DEADLOCK | NOT_DEADLOCK) => {}
        Some(_) => {
            state.cache_update_queries(hash, false, SearchMode::MiniCorral);
            if state.cache_queries(hash, false, SearchMode::MiniCorral) != 100 {
                return Ok(SEARCH_EXCEEDED);
            }
        }
        None => {
            state.cache_insert(hash, SEARCH_EXCEEDED, false, SearchMode::MiniCorral);
            return Ok(SEARCH_EXCEEDED);
        }
    }

    let previous = state.suppress_cache_writes;
    state.suppress_cache_writes |= hidden;
    let result = mini_corral_evaluate(ctx, state, board, hash);
    state.suppress_cache_writes = previous;
    result
}

fn mini_corral_evaluate(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    hash: u64,
) -> Result<i8> {
    if board.all_boxes_in_place(false, &ctx.level.initial) {
        state.cache_insert(hash, NOT_DEADLOCK, false, SearchMode::MiniCorral);
        return Ok(NOT_DEADLOCK);
    }

    let res = can_put_all_on_targets(ctx, state, board, 1000)?;
    if res == 0 {
        if state.learn_mini_corral {
            let mut c = *board;
            c.clear_deadlock_zone_inplace();
            crate::deadlock::stuck::register_mini_corral(ctx, state, &c)?;
        }
        state.cache_insert(hash, IS_DEADLOCK, false, SearchMode::MiniCorral);
        Ok(IS_DEADLOCK)
    } else {
        state.cache_insert(hash, NOT_DEADLOCK, false, SearchMode::MiniCorral);
        Ok(NOT_DEADLOCK)
    }
}

fn should_reevaluate(queries: u32) -> bool {
    if queries < (MAX_DEADLOCK_SEARCH as u32 * 2) {
        return false;
    }
    queries & (queries - 1) == 0
}

pub fn advanced_deadlock_search(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
) -> Result<i8> {
    let hash = board.hash();
    let hidden = state.cache_hit_is_hidden(hash, pull_mode, SearchMode::DeadlockSearch);
    let mut res = state
        .cache_get(hash, pull_mode, SearchMode::DeadlockSearch)
        .unwrap_or(-1);
    state.cache_update_queries(hash, pull_mode, SearchMode::DeadlockSearch);

    if res == SEARCH_EXCEEDED {
        let queries = state
            .cache
            .queries(hash, pull_mode, SearchMode::DeadlockSearch);
        if should_reevaluate(queries) {
            let previous = state.suppress_cache_writes;
            state.suppress_cache_writes |= hidden;
            let outcome =
                corral_deadlock_search_actual(ctx, state, board, pull_mode, queries as usize);
            state.suppress_cache_writes = previous;
            res = outcome?;
            if res != SEARCH_EXCEEDED {
                state.cache_insert(hash, res, pull_mode, SearchMode::DeadlockSearch);
            }
        }
    }

    if (0..=2).contains(&res) {
        return Ok(res);
    }

    let previous = state.suppress_cache_writes;
    state.suppress_cache_writes |= hidden;
    let outcome = corral_deadlock_search_actual(ctx, state, board, pull_mode, MAX_DEADLOCK_SEARCH);
    state.suppress_cache_writes = previous;
    let res = outcome?;
    state.cache_insert(hash, res, pull_mode, SearchMode::DeadlockSearch);
    Ok(res)
}

pub fn advanced_deadlock_pull_mode(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    without_wobblers: bool,
) -> Result<bool> {
    let mut b = *board_in;
    b.clear_bases_inplace();
    if crate::bfs::get_connectivity(&ctx.level, &b) == 1 {
        return Ok(false);
    }

    if without_wobblers {
        remove_wobblers(&ctx.level, &mut b, true);
    }

    set_deadlock_zone_in_pull_mode(&ctx.level, &mut b);

    let mut c = b;
    clear_boxes_out_of_deadlock_zone(&mut c);

    if c.boxes_in_level() == ctx.level.box_count {
        return Ok(false);
    }

    let res = advanced_deadlock_search(ctx, state, &c, true)?;
    if res == NOT_DEADLOCK || res == SEARCH_EXCEEDED {
        return Ok(false);
    }

    if !work_left_outside_deadlock_zone(&ctx.level, &b) {
        return Ok(false);
    }
    Ok(true)
}

fn get_components_around_place(
    ctx: &LevelContext,
    y: usize,
    x: usize,
    dist: &[u32; MAX_INNER],
    list: &mut [u32; 4],
) -> usize {
    let mut n = 0;
    for k in 0..4 {
        let ny = y as isize + DELTA_Y[k];
        let nx = x as isize + DELTA_X[k];
        if ny < 0 || nx < 0 {
            continue;
        }
        let (ny, nx) = (ny as usize, nx as usize);
        if !ctx.level.initial.in_bounds(ny, nx) {
            continue;
        }
        let value = dist[Board::flat_index(ny, nx)];
        if value == INF {
            continue;
        }
        list[n] = value;
        n += 1;
    }
    n
}

fn keep_two_components(
    ctx: &LevelContext,
    board: &mut Board,
    dist: &[u32; MAX_INNER],
    comp1: u32,
    comp2: u32,
    pull_mode: bool,
) {
    let mut group = Board::new(board.width(), board.height()).expect("same dims");
    for y in 0..board.height() {
        for x in 0..board.width() {
            let value = dist[Board::flat_index(y, x)];
            if value == comp1 || value == comp2 {
                group.set(y, x, 1);
            }
        }
    }
    init_deadlock_zone_from_group(&ctx.level, board, &group, pull_mode);
}

pub fn corral_deadlock(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    pull_mode: bool,
) -> Result<bool> {
    corral_deadlock_prepared(ctx, state, board_in, None, pull_mode)
}

pub(crate) fn corral_deadlock_prepared(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    wallified: Option<&Board>,
    pull_mode: bool,
) -> Result<bool> {
    if board_in.is_solved(pull_mode, &ctx.level.initial, ctx.level.initial_sokoban) {
        return Ok(false);
    }

    if pull_mode {
        return advanced_deadlock_pull_mode(ctx, state, board_in, false);
    }

    let b = match wallified {
        Some(board) => *board,
        None => crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
            &ctx.level,
            &ctx.distances,
            board_in,
            &state.envelopes,
        ),
    };

    let mut dist = [INF; MAX_INNER];
    let connect_num = crate::bfs::mark_connectivities(&ctx.level, &b, &mut dist);
    if connect_num >= MAX_SIZE {
        return Ok(false);
    }

    #[allow(clippy::useless_vec)]
    let mut checked = vec![vec![false; MAX_SIZE]; MAX_SIZE];
    #[allow(clippy::useless_vec)]
    let mut search_exceeded = vec![false; MAX_SIZE];

    let Some((sok_y, sok_x)) = b.sokoban_position() else {
        return Ok(false);
    };
    let sokoban_comp = dist[Board::flat_index(sok_y, sok_x)];

    for c1 in 0..connect_num {
        if c1 as u32 == sokoban_comp {
            continue;
        }
        checked[c1][c1] = true;
        let mut c = b;
        keep_two_components(ctx, &mut c, &dist, c1 as u32, c1 as u32, false);
        clear_boxes_out_of_deadlock_zone(&mut c);
        clear_frozen_boxes_away_from_deadlock_zone(&ctx.level, &mut c);

        let res = advanced_deadlock_search(ctx, state, &c, false)?;
        if res == IS_DEADLOCK {
            return Ok(true);
        }
        if res == SEARCH_EXCEEDED {
            search_exceeded[c1] = true;
        }

        let res = mini_corral(ctx, state, &c)?;
        if res == IS_DEADLOCK {
            return Ok(true);
        }
    }

    let mut list = [0u32; 4];
    for y in 0..b.height() {
        for x in 0..b.width() {
            if b.cell(y, x) & BOX == 0 {
                continue;
            }
            let list_size = get_components_around_place(ctx, y, x, &dist, &mut list);
            for k1 in 0..list_size {
                for k2 in k1..list_size {
                    let c1 = list[k1] as usize;
                    let c2 = list[k2] as usize;
                    if c1 >= MAX_SIZE || c2 >= MAX_SIZE {
                        continue;
                    }
                    if c1 as u32 == sokoban_comp || c2 as u32 == sokoban_comp {
                        continue;
                    }
                    if checked[c1][c2] || search_exceeded[c1] || search_exceeded[c2] {
                        continue;
                    }
                    checked[c1][c2] = true;
                    checked[c2][c1] = true;

                    let mut c = b;
                    keep_two_components(ctx, &mut c, &dist, c1 as u32, c2 as u32, false);
                    clear_boxes_out_of_deadlock_zone(&mut c);
                    clear_frozen_boxes_away_from_deadlock_zone(&ctx.level, &mut c);

                    let res = advanced_deadlock_search(ctx, state, &c, false)?;
                    if res == IS_DEADLOCK {
                        return Ok(true);
                    }

                    let res = mini_corral(ctx, state, &c)?;
                    if res == IS_DEADLOCK {
                        return Ok(true);
                    }
                }
            }
        }
    }

    Ok(false)
}
