// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Wobblers deadlock search (`wobblers.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::positions::PositionSet;
use crate::deadlock::state::DeadlockState;
use crate::deadlock::{IS_DEADLOCK, NOT_DEADLOCK, SEARCH_EXCEEDED};
use crate::error::Result;
use crate::level::Level;
use crate::moves::Move;
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

const MAX_WOBBLERS_DEADLOCK_SEARCH: usize = 50;

pub fn reduce_moves_in_wobblers_search(
    ctx: &LevelContext,
    moves: &mut [Move],
    moves_num: usize,
) -> usize {
    for i in 0..moves_num {
        let mut kill = moves[i].attr.reversible;
        let (from_y, from_x) = ctx.level.coord(moves[i].from as usize);
        let (to_y, to_x) = ctx.level.coord(moves[i].to as usize);
        if from_y.abs_diff(to_y) >= 2 && from_x.abs_diff(to_x) >= 2 {
            kill = true;
        }
        if kill {
            moves[0] = moves[i];
            moves[0].kill = true;
            return 1;
        }
    }
    moves_num
}

fn box_can_be_pulled(board: &Board, y: usize, x: usize) -> bool {
    for i in 0..4 {
        let ny = y as isize + DELTA_Y[i];
        let nx = x as isize + DELTA_X[i];
        if board.get(ny, nx) & SOKOBAN != 0
            && board.get(y as isize + DELTA_Y[i] * 2, x as isize + DELTA_X[i] * 2) & SOKOBAN != 0
        {
            return true;
        }
    }
    false
}

pub fn remove_boxes_with_no_player_access(board: &mut Board, with_diag: bool) {
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            let mut touched = 0;
            let mut untouched = 0;

            for k in 0..8 {
                if !with_diag && k & 1 != 0 {
                    continue;
                }
                let ny = y as isize + DELTA_Y_8[k];
                let nx = x as isize + DELTA_X_8[k];
                let val = board.get(ny, nx);
                if val & SOKOBAN != 0 {
                    touched += 1;
                }
                if val & OCCUPIED == 0 && val & SOKOBAN == 0 {
                    untouched += 1;
                }
            }

            if touched == 0 {
                board.and_not(y, x, BOX);
                continue;
            }
            if untouched > 0 && box_can_be_pulled(board, y, x) {
                board.and_not(y, x, BOX);
            }
        }
    }
    board.expand_sokoban_cloud();
}

fn push_is_reversible(board_in: &Board, y: usize, x: usize, d: usize) -> bool {
    let dy = DELTA_Y[d];
    let dx = DELTA_X[d];
    if board_in.get(y as isize - dy, x as isize - dx) & SOKOBAN == 0 {
        return false;
    }
    if board_in.get(y as isize + dy, x as isize + dx) & OCCUPIED != 0 {
        return false;
    }
    if board_in.get(y as isize + dy * 2, x as isize + dx * 2) & OCCUPIED != 0 {
        return false;
    }

    let mut b = *board_in;
    b.and_not(y, x, BOX);
    b.or((y as isize + dy) as usize, (x as isize + dx) as usize, BOX);
    b.clear_sokoban_inplace();
    b.or(y, x, SOKOBAN);
    b.expand_sokoban_cloud();
    b.get(y as isize + dy * 2, x as isize + dx * 2) & SOKOBAN != 0
}

fn pull_is_reversible(board_in: &Board, y: usize, x: usize, d: usize) -> bool {
    let dy = DELTA_Y[d];
    let dx = DELTA_X[d];
    if board_in.get(y as isize + dy, x as isize + dx) & SOKOBAN == 0 {
        return false;
    }
    if board_in.get(y as isize - dy, x as isize - dx) & OCCUPIED != 0 {
        return false;
    }
    if board_in.get(y as isize + dy * 2, x as isize + dx * 2) & OCCUPIED != 0 {
        return false;
    }

    let mut b = *board_in;
    b.and_not(y, x, BOX);
    b.or((y as isize + dy) as usize, (x as isize + dx) as usize, BOX);
    b.clear_sokoban_inplace();
    b.or(
        (y as isize + dy * 2) as usize,
        (x as isize + dx * 2) as usize,
        SOKOBAN,
    );
    b.expand_sokoban_cloud();
    b.cell(y, x) & SOKOBAN != 0
}

pub fn remove_wobblers(level: &Level, board: &mut Board, pull_mode: bool) {
    let mut changed = true;
    while changed {
        changed = false;
        for y in 0..board.height() {
            for x in 0..board.width() {
                if board.cell(y, x) & BOX == 0 {
                    continue;
                }
                for k in 0..4 {
                    let reversible = if pull_mode {
                        pull_is_reversible(board, y, x, k)
                    } else {
                        push_is_reversible(board, y, x, k)
                    };
                    if !reversible {
                        continue;
                    }
                    board.and_not(y, x, BOX);
                    board.or(y, x, SOKOBAN);
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            board.expand_sokoban_cloud();
        }
    }
    let _ = level;
}

fn is_better_wobblers_score(new_score: &ScoreElement, old_score: &ScoreElement) -> bool {
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

fn get_best_wobblers_move(positions: &PositionSet) -> Option<(usize, usize)> {
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
            if best.is_none() || is_better_wobblers_score(&p.scores[j], &best_score) {
                best = Some((i, j));
                best_score = p.scores[j];
            }
        }
    }
    best
}

fn wobblers_search_actual(
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
        SearchMode::WobblersSearch,
        &mut helper,
        state,
    )?;

    let result = loop {
        if positions.estimate_uniqe_moves() > max_positions {
            break SEARCH_EXCEEDED;
        }
        let Some((pos, son)) = get_best_wobblers_move(&positions) else {
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

fn wobbler_deadlock_search(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
) -> Result<i8> {
    let hash = board.hash();
    if let Some(res) = state.cache_get(hash, pull_mode, SearchMode::WobblersSearch) {
        if (0..=2).contains(&res) {
            return Ok(res);
        }
    }

    let hidden = state.cache_hit_is_hidden(hash, pull_mode, SearchMode::WobblersSearch);
    let previous = state.suppress_cache_writes;
    state.suppress_cache_writes |= hidden;
    let outcome =
        wobblers_search_actual(ctx, state, board, pull_mode, MAX_WOBBLERS_DEADLOCK_SEARCH);
    state.suppress_cache_writes = previous;
    let res = outcome?;
    state.cache_insert(hash, res, pull_mode, SearchMode::WobblersSearch);
    Ok(res)
}

pub fn is_wobblers_deadlock(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    pull_mode: bool,
) -> Result<bool> {
    if !pull_mode {
        return Ok(false);
    }

    let mut b = *board_in;
    b.clear_bases_inplace();
    remove_boxes_with_no_player_access(&mut b, false);
    remove_wobblers(&ctx.level, &mut b, pull_mode);

    let n = b.boxes_in_level();
    if n == 0 || n == ctx.level.box_count {
        return Ok(false);
    }

    Ok(wobbler_deadlock_search(ctx, state, &b, pull_mode)? == IS_DEADLOCK)
}
