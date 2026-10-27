// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! XY deadlock search (`xy_deadlock.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::positions::PositionSet;
use crate::deadlock::state::DeadlockState;
use crate::deadlock::{IS_DEADLOCK, NOT_DEADLOCK, SEARCH_EXCEEDED};
use crate::error::Result;
use crate::moves::{Move, apply_move, find_possible_moves};
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

const MAX_XY_DEADLOCK_SEARCH: usize = 500;

fn mark_hv_indices(ctx: &LevelContext, board: &Board, moves: &[Move]) -> Vec<bool> {
    let mut con_ok = vec![false; moves.len()];
    for (i, mv) in moves.iter().enumerate() {
        let mut c = *board;
        apply_move(&mut c, mv, SearchMode::Normal, &ctx.level);
        let (from_y, from_x) = ctx.level.coord(mv.from as usize);
        if c.cell(from_y, from_x) & SOKOBAN != 0 {
            con_ok[i] = true;
        }
    }

    let mut h_indices = vec![false; ctx.level.index_num()];
    let mut v_indices = vec![false; ctx.level.index_num()];
    for (i, mv) in moves.iter().enumerate() {
        if !con_ok[i] {
            continue;
        }
        let (from_y, from_x) = ctx.level.coord(mv.from as usize);
        let (to_y, to_x) = ctx.level.coord(mv.to as usize);
        if to_y != from_y {
            v_indices[mv.from as usize] = true;
        }
        if to_x != from_x {
            h_indices[mv.from as usize] = true;
        }
    }

    h_indices
        .into_iter()
        .zip(v_indices)
        .map(|(h, v)| h && v)
        .collect()
}

pub fn reduce_moves_in_xy_search(
    ctx: &LevelContext,
    board: &Board,
    moves: &mut [Move],
    moves_num: usize,
) -> usize {
    let hv = mark_hv_indices(ctx, board, &moves[..moves_num]);
    for i in 0..moves_num {
        if hv[moves[i].from as usize] {
            moves[0] = moves[i];
            moves[0].kill = true;
            return 1;
        }
    }
    moves_num
}

fn is_better_xy_score(new_score: &ScoreElement, old_score: &ScoreElement) -> bool {
    if new_score.boxes_in_level < old_score.boxes_in_level {
        return true;
    }
    if new_score.boxes_in_level > old_score.boxes_in_level {
        return false;
    }
    if new_score.connectivity < old_score.connectivity {
        return true;
    }
    if new_score.connectivity > old_score.connectivity {
        return false;
    }
    if new_score.boxes_on_targets > old_score.boxes_on_targets {
        return true;
    }
    if new_score.boxes_on_targets < old_score.boxes_on_targets {
        return false;
    }
    false
}

fn get_best_xy_move(positions: &PositionSet) -> Option<(usize, usize)> {
    let mut best = None;
    let mut best_score = ScoreElement::default();
    for i in 0..positions.len() {
        let p = &positions.positions[i];
        if p.position_deadlocked {
            continue;
        }
        for j in 0..p.moves.len() {
            if p.move_deadlocked[j] || p.place_in_positions[j].is_some() {
                continue;
            }
            if best.is_none() || is_better_xy_score(&p.scores[j], &best_score) {
                best = Some((i, j));
                best_score = p.scores[j];
            }
        }
    }
    best
}

fn xy_search_actual(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
    max_positions: usize,
) -> Result<i8> {
    let mut helper = Helper::default();
    if board.all_boxes_in_place(pull_mode, &ctx.level.initial) {
        return Ok(NOT_DEADLOCK);
    }

    let mut positions = PositionSet::new(max_positions);
    positions.set_first_position(
        ctx,
        board,
        pull_mode,
        SearchMode::XySearch,
        &mut helper,
        state,
    )?;

    let result = loop {
        if positions.estimate_uniqe_moves() > max_positions {
            break SEARCH_EXCEEDED;
        }
        let Some((pos, son)) = get_best_xy_move(&positions) else {
            break IS_DEADLOCK;
        };
        positions.expand_position(ctx, pos, son, pull_mode, &mut helper, state)?;
        let last = positions.len() - 1;
        if positions.positions[last]
            .board
            .all_boxes_in_place(pull_mode, &ctx.level.initial)
        {
            break NOT_DEADLOCK;
        }
        if positions.len() == max_positions {
            break SEARCH_EXCEEDED;
        }
    };

    Ok(result)
}

pub fn xy_deadlock_search(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
) -> Result<i8> {
    let hash = board.hash();
    if let Some(res) = state.cache_get(hash, pull_mode, SearchMode::XySearch) {
        if (0..=2).contains(&res) {
            return Ok(res);
        }
    }

    let hidden = state.cache_hit_is_hidden(hash, pull_mode, SearchMode::XySearch);
    let previous = state.suppress_cache_writes;
    state.suppress_cache_writes |= hidden;
    let outcome = xy_search_actual(ctx, state, board, pull_mode, MAX_XY_DEADLOCK_SEARCH);
    state.suppress_cache_writes = previous;
    let res = outcome?;
    state.cache_insert(hash, res, pull_mode, SearchMode::XySearch);
    Ok(res)
}

pub fn clear_xy_boxes(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &mut Board,
    pull_mode: bool,
) -> Result<()> {
    let mut helper = Helper::default();
    loop {
        let (moves, _) = find_possible_moves(
            ctx,
            board,
            pull_mode,
            SearchMode::XySearch,
            &mut helper,
            state,
        )?;
        if !moves.is_empty() && moves[0].kill {
            let (y, x) = ctx.level.coord(moves[0].from as usize);
            board.and_not(y, x, BOX);
            board.expand_sokoban_cloud();
        } else {
            return Ok(());
        }
    }
}

pub fn is_xy_deadlock(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    pull_mode: bool,
) -> Result<bool> {
    let b = if pull_mode {
        let mut c = *board_in;
        c.clear_bases_inplace();
        c
    } else {
        crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
            &ctx.level,
            &ctx.distances,
            board_in,
            &state.envelopes,
        )
    };

    Ok(xy_deadlock_search(ctx, state, &b, pull_mode)? == IS_DEADLOCK)
}
