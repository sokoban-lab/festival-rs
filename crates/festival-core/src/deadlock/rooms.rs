// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Room deadlock search (`rooms_deadlock.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::positions::PositionSet;
use crate::deadlock::state::{DeadlockState, RoomsDeadlockData};
use crate::deadlock::{IS_DEADLOCK, NOT_DEADLOCK, SEARCH_EXCEEDED};
use crate::error::Result;
use crate::moves::Move;
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

const MAX_ROOMS_DEADLOCK_SEARCH: usize = 200;

pub fn init_rooms_deadlock(ctx: &LevelContext) -> RoomsDeadlockData {
    let level = &ctx.level;
    let mut b = level.initial;
    b.clear_boxes_inplace();

    let mut room_with_corridors = Vec::with_capacity(ctx.rooms.num);
    let mut room_size = Vec::with_capacity(ctx.rooms.num);

    for t in 0..ctx.rooms.num {
        let mut room = Board::new(b.width(), b.height()).expect("same dims");
        for y in 0..b.height() {
            for x in 0..b.width() {
                if ctx.rooms.color(y, x) != t as u32 {
                    continue;
                }
                room.set(y, x, 1);
                for k in 0..4 {
                    let ny = y as isize + DELTA_Y[k];
                    let nx = x as isize + DELTA_X[k];
                    if ny < 0 || nx < 0 {
                        continue;
                    }
                    let (ny, nx) = (ny as usize, nx as usize);
                    if !b.in_bounds(ny, nx) {
                        continue;
                    }
                    if b.cell(ny, nx) & OCCUPIED == 0 {
                        room.set(ny, nx, 1);
                    }
                }
            }
        }
        let size = room
            .cells()
            .iter()
            .take(MAX_INNER)
            .filter(|value| **value != 0)
            .count();
        room_with_corridors.push(room);
        room_size.push(size);
    }

    RoomsDeadlockData {
        room_with_corridors,
        room_size,
    }
}

pub fn reduce_moves_in_rooms_deadlock_search(
    ctx: &LevelContext,
    state: &DeadlockState,
    moves: &mut [Move],
    moves_num: usize,
    tested_deadlock_room: usize,
) -> usize {
    for i in 0..moves_num {
        let (y, x) = ctx.level.coord(moves[i].to as usize);
        let room = ctx.rooms.color(y, x);
        if room == INF {
            continue;
        }
        if room as usize != tested_deadlock_room {
            moves[0] = moves[i];
            moves[0].kill = true;
            return 1;
        }
    }
    let _ = state;
    moves_num
}

fn boxes_in_room(board: &Board, data: &RoomsDeadlockData, room: usize) -> usize {
    let mut sum = 0;
    let area = &data.room_with_corridors[room];
    for y in 0..board.height() {
        for x in 0..board.width() {
            if area.cell(y, x) != 0 && board.cell(y, x) & BOX != 0 {
                sum += 1;
            }
        }
    }
    sum
}

fn is_better_rooms_deadlock_score(new_score: &ScoreElement, old_score: &ScoreElement) -> bool {
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

fn get_best_rooms_deadlock_move(positions: &PositionSet) -> Option<(usize, usize)> {
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
            if best.is_none() || is_better_rooms_deadlock_score(&p.scores[j], &best_score) {
                best = Some((i, j));
                best_score = p.scores[j];
            }
        }
    }
    best
}

fn rooms_deadlock_search_actual(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
    tested_deadlock_room: usize,
) -> Result<i8> {
    let mut helper = Helper {
        tested_deadlock_room,
        ..Helper::default()
    };
    if board.all_boxes_in_place(pull_mode, &ctx.level.initial) {
        return Ok(NOT_DEADLOCK);
    }

    let mut positions = PositionSet::new(MAX_ROOMS_DEADLOCK_SEARCH);
    positions.set_first_position(
        ctx,
        board,
        pull_mode,
        SearchMode::RoomsDeadlockSearch,
        &mut helper,
        state,
    )?;

    let result = loop {
        if positions.estimate_uniqe_moves() > MAX_ROOMS_DEADLOCK_SEARCH {
            break SEARCH_EXCEEDED;
        }
        let Some((pos, son)) = get_best_rooms_deadlock_move(&positions) else {
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
        if positions.len() == MAX_ROOMS_DEADLOCK_SEARCH {
            break SEARCH_EXCEEDED;
        }
    };

    Ok(result)
}

fn rooms_deadlock_search(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
    tested_deadlock_room: usize,
) -> Result<i8> {
    let hash = board.hash();
    if let Some(res) = state.cache_get(hash, pull_mode, SearchMode::RoomsDeadlockSearch) {
        if (0..=2).contains(&res) {
            return Ok(res);
        }
    }

    let hidden = state.cache_hit_is_hidden(hash, pull_mode, SearchMode::RoomsDeadlockSearch);
    let previous = state.suppress_cache_writes;
    state.suppress_cache_writes |= hidden;
    let outcome = rooms_deadlock_search_actual(ctx, state, board, pull_mode, tested_deadlock_room);
    state.suppress_cache_writes = previous;
    let res = outcome?;
    state.cache_insert(hash, res, pull_mode, SearchMode::RoomsDeadlockSearch);
    Ok(res)
}

fn keep_boxes_in_room(board: &mut Board, data: &RoomsDeadlockData, room: usize) {
    let area = &data.room_with_corridors[room];
    for y in 0..board.height() {
        for x in 0..board.width() {
            if area.cell(y, x) == 0 {
                board.and_not(y, x, BOX);
            }
        }
    }
    board.expand_sokoban_cloud();
}

pub fn is_rooms_deadlock(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    pull_mode: bool,
    last_move: &Move,
) -> Result<bool> {
    if last_move.base || last_move.kill {
        return Ok(false);
    }
    if ctx.rooms.num == 1 {
        return Ok(false);
    }

    let (y, x) = ctx.level.coord(last_move.to as usize);
    let mut b = *board_in;
    if pull_mode {
        b.clear_bases_inplace();
    }

    for t in 0..ctx.rooms.num {
        if state.rooms.room_with_corridors[t].cell(y, x) == 0 {
            continue;
        }
        if state.rooms.room_size[t] >= 25 {
            continue;
        }
        let n = boxes_in_room(&b, &state.rooms, t);
        if n <= 1 || n > 5 {
            continue;
        }
        if n == 5 && state.rooms.room_size[t] > 17 {
            continue;
        }
        if n == 4 && state.rooms.room_size[t] > 20 {
            continue;
        }

        let mut c = b;
        keep_boxes_in_room(&mut c, &state.rooms, t);
        if rooms_deadlock_search(ctx, state, &c, pull_mode, t)? == IS_DEADLOCK {
            return Ok(true);
        }
    }
    Ok(false)
}
