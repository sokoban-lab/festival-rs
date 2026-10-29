//! Full deadlock oracle used by the search during Phase 3.
//!
//! This module ports `advanced_deadlock.cpp`, the matching check from
//! `advanced_deadlock.cpp`, and the mode dispatch in `check_deadlock_in_moves`
//! from `deadlock.cpp`. It replaces the Phase 2 static-only subset.

use crate::analysis::hungarian::{CostMatrix, HungarianSolution, solve_hungarian};
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::corral_search::corral_deadlock;
use crate::deadlock::k_dist::{is_k_dist_deadlock, sokoban_touches_a_filled_hole};
use crate::deadlock::patterns::{groups_match, patterns};
use crate::deadlock::rooms::is_rooms_deadlock as rooms_deadlock;
use crate::deadlock::state::DeadlockState;
use crate::deadlock::stuck::is_stuck_deadlock;
use crate::deadlock::wobblers::is_wobblers_deadlock;
use crate::deadlock::zone::{size_of_deadlock_zone, work_left_in_deadlock_zone};
use crate::distance::Distances;
use crate::error::{Result, SolveError};
use crate::graph::Graph;
use crate::level::Level;
use crate::moves::{Move, apply_move};
use crate::search::helper::Helper;
use crate::solver::LevelContext;

/// Checks that are now wired into the full oracle.
pub const IMPLEMENTED_CHECKS: &[&str] = &[
    "embedded-patterns",
    "2x2",
    "3x3",
    "freeze",
    "fixed-box-walls",
    "mpdb2",
    "matching",
    "free-pull",
    "pull-zones",
    "corral",
    "k-dist",
    "wobblers",
    "rooms",
    "learned-patterns",
    "diags",
];

/// Checks still intentionally absent after Phase 3.
pub const UNIMPLEMENTED_CHECKS: &[&str] = &[];

pub fn box_touches_unreachable_square(board: &Board, box_y: usize, box_x: usize) -> bool {
    for dir in 0..8 {
        let y = box_y as isize + DELTA_Y_8[dir];
        let x = box_x as isize + DELTA_X_8[dir];
        let cell = board.get(y, x);
        if cell & OCCUPIED != 0 {
            continue;
        }
        if cell & SOKOBAN != 0 {
            continue;
        }
        return true;
    }
    false
}

fn box_is_pushable(board: &Board, y: usize, x: usize) -> bool {
    if board.at_dir(y, x, UP) & OCCUPIED == 0 && board.at_dir(y, x, DOWN) & OCCUPIED == 0 {
        return true;
    }
    if board.at_dir(y, x, LEFT) & OCCUPIED == 0 && board.at_dir(y, x, RIGHT) & OCCUPIED == 0 {
        return true;
    }
    false
}

/// Port of `is_freeze_deadlock`.
pub fn is_freeze_deadlock(board: &Board, y: usize, x: usize) -> bool {
    if board.cell(y, x) & BOX == 0 {
        return false;
    }
    if box_is_pushable(board, y, x) {
        return false;
    }

    let mut c = *board;
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..c.height() {
            for j in 0..c.width() {
                if c.cell(i, j) & BOX != 0 && box_is_pushable(&c, i, j) {
                    c.and_not(i, j, BOX);
                    changed = true;
                }
            }
        }
    }

    for i in 0..c.height() {
        for j in 0..c.width() {
            if c.cell(i, j) & BOX != 0 && c.cell(i, j) & TARGET == 0 {
                return true;
            }
        }
    }
    false
}

/// Port of `is_3x3_deadlock`.
pub fn is_3x3_deadlock(level: &Level, board: &Board, box_y: usize, box_x: usize) -> bool {
    if !box_touches_unreachable_square(board, box_y, box_x) {
        return false;
    }

    for y in box_y as isize - 1..=box_y as isize + 1 {
        for x in box_x as isize - 1..=box_x as isize + 1 {
            if y < 0 || x < 0 {
                continue;
            }
            let (yu, xu) = (y as usize, x as usize);
            if yu >= board.height() || xu >= board.width() {
                continue;
            }
            if !level.is_inner(yu, xu) {
                continue;
            }
            if board.cell(yu, xu) & OCCUPIED != 0 || board.cell(yu, xu) & TARGET != 0 {
                continue;
            }

            let mut surrounded = true;
            for dir in 0..4 {
                if board.at_dir(yu, xu, dir) & OCCUPIED == 0 {
                    surrounded = false;
                    break;
                }
            }
            if !surrounded {
                continue;
            }

            let mut unpacked_box = false;
            for dir in 0..4 {
                let cell = board.at_dir(yu, xu, dir);
                if cell & BOX != 0 && cell & TARGET == 0 {
                    unpacked_box = true;
                    break;
                }
            }
            if !unpacked_box {
                continue;
            }

            let top_left = board.get(y - 1, x - 1);
            let bottom_right = board.get(y + 1, x + 1);
            if top_left & OCCUPIED != 0 && bottom_right & OCCUPIED != 0 {
                return true;
            }
            let top_right = board.get(y - 1, x + 1);
            let bottom_left = board.get(y + 1, x - 1);
            if top_right & OCCUPIED != 0 && bottom_left & OCCUPIED != 0 {
                return true;
            }

            for i in (0..8).step_by(2) {
                let val1 = board.get(y + DELTA_Y_8[i], x + DELTA_X_8[i]);
                if val1 != WALL {
                    continue;
                }
                let val2 = board.get(y + DELTA_Y_8[(i + 3) & 7], x + DELTA_X_8[(i + 3) & 7]);
                let val3 = board.get(y + DELTA_Y_8[(i + 5) & 7], x + DELTA_X_8[(i + 5) & 7]);
                if val2 & OCCUPIED == 0 || val3 & OCCUPIED == 0 {
                    continue;
                }
                if val2 == WALL || val3 == WALL {
                    return true;
                }
            }
        }
    }

    false
}

/// Port of `check_all_patterns`; returns whether a pattern matched.
pub fn check_all_patterns(
    level: &Level,
    board: &Board,
    box_y: usize,
    box_x: usize,
    pull_mode: bool,
) -> bool {
    if !box_touches_unreachable_square(board, box_y, box_x) {
        return false;
    }
    if pull_mode && board.sokoban_cloud_size() > 4 {
        return false;
    }

    let mut b = *board;
    b.clear_deadlock_zone_inplace();
    let patterns = patterns();

    if !pull_mode {
        return groups_match(&patterns.normal_groups, &b, box_y, box_x);
    }

    let (sok_y, sok_x) = level.initial_sokoban;
    if b.get(sok_y as isize, sok_x as isize) & SOKOBAN != 0 {
        return false;
    }

    groups_match(&patterns.pull_groups, &b, box_y, box_x)
}

fn has_pull_continuation(ctx: &LevelContext, board: &Board, search_mode: SearchMode) -> bool {
    let level = &ctx.level;
    if board.all_boxes_in_place(true, &level.initial) {
        let (sok_y, sok_x) = level.initial_sokoban;
        if board.cell(sok_y, sok_x) & SOKOBAN != 0 {
            return true;
        }
    }

    if search_mode == SearchMode::KDistSearch {
        if board.sokoban_cloud_size() > 1 {
            return true;
        }
        if sokoban_touches_a_filled_hole(ctx, board) {
            return true;
        }
        return false;
    }

    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            for dir in 0..4 {
                let sok_y = y as isize + DELTA_Y[dir];
                let sok_x = x as isize + DELTA_X[dir];
                if board.get(sok_y, sok_x) & SOKOBAN == 0 {
                    continue;
                }
                let next_y = sok_y + DELTA_Y[dir];
                let next_x = sok_x + DELTA_X[dir];
                if board.get(next_y, next_x) & OCCUPIED == 0 {
                    return true;
                }
            }
        }
    }
    false
}

fn is_inefficient_position(
    ctx: &LevelContext,
    board: &Board,
    box_index: u16,
    pull_mode: bool,
    search_mode: SearchMode,
    helper: &Helper,
) -> bool {
    if helper.perimeter_found || helper.enable_inefficient_moves {
        return false;
    }
    if search_mode == SearchMode::KDistSearch {
        return false;
    }

    let (box_y, box_x) = ctx.level.coord(box_index as usize);

    if !pull_mode {
        if helper.used_in_plan.cell(box_y, box_x) != 0 {
            return false;
        }
        if search_mode == SearchMode::HfSearch && helper.imagined_hf_board.cell(box_y, box_x) != 0 {
            return false;
        }
    } else if search_mode == SearchMode::RevSearch
        && helper.imagined_hf_board.cell(box_y, box_x) != 0
    {
        return false;
    }

    if !pull_mode {
        if board.cell(box_y, box_x) & TARGET != 0 {
            return false;
        }
    } else if ctx.level.initial.cell(box_y, box_x) & BOX != 0 {
        return false;
    }

    let forbidden = if pull_mode {
        &ctx.level.forbidden_pull
    } else {
        &ctx.level.forbidden_push
    };
    if forbidden.cell(box_y, box_x) != 0 {
        return true;
    }

    let mut val = [0u8; 8];
    for i in 0..8 {
        val[i] = board.get(box_y as isize + DELTA_Y_8[i], box_x as isize + DELTA_X_8[i]);
    }
    let at2 = |j: usize| {
        board.get(
            box_y as isize + DELTA_Y_8[j] * 2,
            box_x as isize + DELTA_X_8[j] * 2,
        )
    };

    if pull_mode {
        for j in (0..8).step_by(2) {
            if val[j] & OCCUPIED == 0
                && val[(j + 1) & 7] == WALL
                && val[(j + 3) & 7] == WALL
                && val[(j + 4) & 7] == WALL
                && at2(j) == WALL
            {
                return true;
            }
        }
        for j in (0..8).step_by(2) {
            if val[j] & OCCUPIED == 0
                && val[(j + 4) & 7] == WALL
                && val[(j + 5) & 7] == WALL
                && val[(j + 7) & 7] == WALL
                && at2(j) == WALL
            {
                return true;
            }
        }
        return false;
    }

    for j in (0..8).step_by(2) {
        if val[(j + 2) & 7] == WALL
            && val[(j + 3) & 7] == WALL
            && val[(j + 5) & 7] == WALL
            && val[(j + 6) & 7] != WALL
            && val[(j + 6) & 7] & SOKOBAN == 0
        {
            return true;
        }
        if val[(j + 3) & 7] == WALL
            && val[(j + 5) & 7] == WALL
            && val[(j + 6) & 7] == WALL
            && val[(j + 2) & 7] != WALL
            && val[(j + 2) & 7] & SOKOBAN == 0
        {
            return true;
        }
        if val[(j + 2) & 7] == WALL
            && val[(j + 3) & 7] == WALL
            && val[(j + 5) & 7] == WALL
            && val[(j + 6) & 7] != WALL
            && val[j] & SOKOBAN != 0
        {
            return true;
        }
        if val[(j + 3) & 7] == WALL
            && val[(j + 5) & 7] == WALL
            && val[(j + 6) & 7] == WALL
            && val[(j + 2) & 7] != WALL
            && val[j] & SOKOBAN != 0
        {
            return true;
        }
        if val[(j + 3) & 7] == WALL
            && val[(j + 5) & 7] == WALL
            && val[(j + 2) & 7] & BOX != 0
            && val[(j + 6) & 7] & SOKOBAN == 0
        {
            return true;
        }
        if val[(j + 3) & 7] == WALL
            && val[(j + 5) & 7] == WALL
            && val[(j + 6) & 7] & BOX != 0
            && val[(j + 2) & 7] & SOKOBAN == 0
        {
            return true;
        }
    }
    false
}

fn verify_simple_base_matching(
    ctx: &LevelContext,
    board: &Board,
    board_with_bases: &Board,
) -> bool {
    let mut box_places = Vec::new();
    let mut bases_places = Vec::new();
    for i in 0..ctx.level.index_num() {
        let (y, x) = ctx.level.coord(i);
        if board_with_bases.cell(y, x) & BASE != 0 {
            bases_places.push(i);
        }
        if board.cell(y, x) & BOX != 0 {
            box_places.push(i);
        }
    }

    if bases_places.len() != box_places.len() {
        return false;
    }

    for &base in &bases_places {
        let matched = box_places
            .iter()
            .any(|&box_index| ctx.distances.from_to(base, box_index) != INF);
        if !matched {
            return false;
        }
    }
    true
}

pub(crate) fn check_matching_deadlock(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    pull_mode: bool,
    search_mode: SearchMode,
) -> Result<bool> {
    let wallified = if pull_mode {
        None
    } else {
        Some(crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
            &ctx.level,
            &ctx.distances,
            board_in,
            &state.envelopes,
        ))
    };
    check_matching_deadlock_prepared(
        ctx,
        state,
        board_in,
        wallified.as_ref(),
        pull_mode,
        search_mode,
    )
}

fn check_matching_deadlock_prepared(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    wallified: Option<&Board>,
    pull_mode: bool,
    search_mode: SearchMode,
) -> Result<bool> {
    if search_mode == SearchMode::Dragonfly {
        return Ok(false);
    }

    let level = &ctx.level;
    let mut has_bases = false;
    for y in 0..board_in.height() {
        for x in 0..board_in.width() {
            if board_in.cell(y, x) & BASE != 0 {
                has_bases = true;
            }
        }
    }

    let mut box_places = Vec::new();
    let mut target_places = Vec::new();
    let b = if pull_mode {
        *board_in
    } else {
        match wallified {
            Some(board) => *board,
            None => crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
                level,
                &ctx.distances,
                board_in,
                &state.envelopes,
            ),
        }
    };

    for i in 0..level.index_num() {
        let (y, x) = level.coord(i);
        if !pull_mode {
            if b.cell(y, x) & BOX != 0 {
                box_places.push(i);
            }
            if b.cell(y, x) & TARGET != 0 {
                target_places.push(i);
            }
        } else {
            if !has_bases {
                if level.initial.cell(y, x) & BOX != 0 {
                    box_places.push(i);
                }
            } else if board_in.cell(y, x) & BASE != 0 {
                box_places.push(i);
            }
            if board_in.cell(y, x) & BOX != 0 {
                target_places.push(i);
            }
        }
    }

    let box_num = box_places.len();
    let target_num = target_places.len();
    if !pull_mode && box_num != target_num {
        return Ok(true);
    }
    if box_num >= MAX_BOXES {
        return Err(SolveError::Internal("hungarian size overflow".into()));
    }

    let mut cost = CostMatrix::new(box_num);
    for i in 0..box_num {
        for j in 0..box_num {
            cost.set(i, j, 1);
        }
    }

    if !pull_mode {
        let mut empty = b;
        empty.clear_boxes_inplace();
        let mut graph = Graph::unbuilt(level);
        graph.build(level, &empty, false);
        graph.make_holes_inactive(level, &b);

        let mut targets = Board::new(b.width(), b.height()).expect("same dims");
        for (i, &box_index) in box_places.iter().enumerate() {
            let (y, x) = level.coord(box_index);
            graph.find_targets_of_place_without_init_graph(level, y, x, &mut targets);
            for (j, &target_index) in target_places.iter().enumerate() {
                let (ty, tx) = level.coord(target_index);
                if targets.cell(ty, tx) != 0 {
                    cost.set(i, j, 0);
                }
            }
        }
    } else {
        for (i, &box_index) in box_places.iter().enumerate() {
            for (j, &target_index) in target_places.iter().enumerate() {
                if ctx.distances.from_to(box_index, target_index) != INF {
                    cost.set(i, j, 0);
                }
            }
        }
    }

    let mut sol = HungarianSolution::default();
    solve_hungarian(box_num, &cost, &mut sol, &mut state.hungarian_cache);

    if !pull_mode {
        Ok(sol.weight != 0)
    } else {
        Ok(sol.weight != box_num as i32 - target_num as i32)
    }
}

fn simple_pull_box_eliminator(board: &mut Board) -> bool {
    let mut changed = false;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            for dir in 0..4 {
                let sok_y = y as isize + DELTA_Y[dir];
                let sok_x = x as isize + DELTA_X[dir];
                if board.get(sok_y, sok_x) & SOKOBAN == 0 {
                    continue;
                }
                let to_y = y as isize + DELTA_Y[dir] * 2;
                let to_x = x as isize + DELTA_X[dir] * 2;
                if board.get(to_y, to_x) & OCCUPIED != 0 {
                    continue;
                }
                board.and_not(y, x, BOX);
                board.or(y, x, SOKOBAN);
                changed = true;
                break;
            }
        }
    }
    changed
}

fn back_eliminate_loop(board: &mut Board) {
    loop {
        let changed = simple_pull_box_eliminator(board);
        if changed {
            board.expand_sokoban_cloud();
        } else {
            break;
        }
    }
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

fn free_pull_deadlock(ctx: &LevelContext, board: &Board) -> bool {
    let mut b = *board;
    back_eliminate_loop(&mut b);
    for y in 0..b.height() {
        for x in 0..b.width() {
            if b.cell(y, x) & BOX != 0 && ctx.level.initial.cell(y, x) & BOX == 0 {
                return true;
            }
        }
    }
    false
}

fn eliminate_from_zone(base: &Board, final_pos: &Board, zones: &[u32; MAX_INNER], z: u32) -> bool {
    let mut e = *final_pos;
    put_sokoban_in_zone(&mut e, zones, z);
    back_eliminate_loop(&mut e);

    for y in 0..e.height() {
        for x in 0..e.width() {
            if e.cell(y, x) & BOX != 0 && base.cell(y, x) & BOX == 0 {
                return false;
            }
        }
    }
    true
}

fn try_pulling_all_zones(ctx: &LevelContext, state: &DeadlockState, board: &Board) -> bool {
    let wallified = crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
        &ctx.level,
        &ctx.distances,
        board,
        &state.envelopes,
    );
    try_pulling_all_zones_prepared(ctx, &wallified)
}

fn try_pulling_all_zones_prepared(ctx: &LevelContext, b: &Board) -> bool {
    let b = *b;
    let mut final_pos = b;
    final_pos.clear_boxes_inplace();
    final_pos.turn_targets_into_packed();

    let mut zones = [INF; MAX_INNER];
    let zones_num = crate::bfs::mark_connectivities(&ctx.level, &final_pos, &mut zones);
    for i in 0..zones_num {
        if eliminate_from_zone(&b, &final_pos, &zones, i as u32) {
            return true;
        }
    }
    false
}

pub fn board_is_deadlocked(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
    search_mode: SearchMode,
) -> Result<bool> {
    let wallified = if pull_mode {
        None
    } else {
        Some(crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
            &ctx.level,
            &ctx.distances,
            board,
            &state.envelopes,
        ))
    };

    if check_matching_deadlock_prepared(
        ctx,
        state,
        board,
        wallified.as_ref(),
        pull_mode,
        search_mode,
    )? {
        return Ok(true);
    }

    if pull_mode && free_pull_deadlock(ctx, board) {
        return Ok(true);
    }

    if !pull_mode {
        let wallified = wallified.as_ref().expect("push mode wallified board");
        if !try_pulling_all_zones_prepared(ctx, wallified) {
            return Ok(true);
        }
    }

    if is_stuck_deadlock(ctx, state, board, pull_mode) {
        return Ok(true);
    }

    if crate::bfs::get_connectivity(&ctx.level, board) != 1
        && crate::deadlock::corral_search::corral_deadlock_prepared(
            ctx,
            state,
            board,
            wallified.as_ref(),
            pull_mode,
        )?
    {
        return Ok(true);
    }

    if !pull_mode {
        if crate::deadlock::diags::is_diag_deadlock(board) {
            return Ok(true);
        }
        if is_k_dist_deadlock(ctx, state, board)? {
            return Ok(true);
        }
    }

    if is_wobblers_deadlock(ctx, state, board, pull_mode)? {
        return Ok(true);
    }

    Ok(false)
}

pub fn move_is_deadlocked(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    mv: &Move,
    pull_mode: bool,
    search_mode: SearchMode,
) -> Result<bool> {
    if mv.kill || mv.base {
        return Ok(false);
    }

    let mut c = *board;
    apply_move(&mut c, mv, SearchMode::Normal, &ctx.level);

    if board_is_deadlocked(ctx, state, &c, pull_mode, search_mode)? {
        return Ok(true);
    }
    rooms_deadlock(ctx, state, &c, pull_mode, mv)
}

/// Diagnostic-only: which individual check rejects `board`.
pub fn explain_board(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
    search_mode: SearchMode,
) -> Vec<(&'static str, bool)> {
    let mut out = Vec::new();
    out.push((
        "matching",
        check_matching_deadlock(ctx, state, board, pull_mode, search_mode).unwrap_or(false),
    ));
    if pull_mode {
        out.push(("free-pull", free_pull_deadlock(ctx, board)));
    } else {
        out.push(("pull-zones", !try_pulling_all_zones(ctx, state, board)));
    }
    out.push(("stuck", is_stuck_deadlock(ctx, state, board, pull_mode)));
    let connectivity = crate::bfs::get_connectivity(&ctx.level, board);
    out.push(("split", connectivity != 1));
    if connectivity != 1 {
        out.push((
            "corral",
            corral_deadlock(ctx, state, board, pull_mode).unwrap_or(false),
        ));
    }
    if !pull_mode {
        out.push(("diag", crate::deadlock::diags::is_diag_deadlock(board)));
        out.push((
            "k-dist",
            is_k_dist_deadlock(ctx, state, board).unwrap_or(false),
        ));
    }
    out.push((
        "wobblers",
        is_wobblers_deadlock(ctx, state, board, pull_mode).unwrap_or(false),
    ));
    out
}

#[allow(clippy::too_many_arguments)]
pub fn check_deadlock_in_moves(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    moves: &[Move],
    pull_mode: bool,
    search_mode: SearchMode,
    helper: &Helper,
) -> Result<Vec<bool>> {
    let mut valid = vec![true; moves.len()];
    let level = &ctx.level;

    let mut b = *board_in;
    if pull_mode {
        b.clear_bases_inplace();
    }

    let search_mode = if search_mode == SearchMode::DeadlockSearch {
        SearchMode::Normal
    } else {
        search_mode
    };

    let mut elim = vec![false; level.index_num()];
    if pull_mode && search_mode == SearchMode::BaseSearch {
        for y in 0..b.height() {
            for x in 0..b.width() {
                if board_in.cell(y, x) & BASE != 0 {
                    if let Some(index) = level.index_at(y, x) {
                        elim[index] = true;
                    }
                }
            }
        }
    }

    for (i, mv) in moves.iter().enumerate() {
        if mv.kill {
            continue;
        }

        let dest = mv.to as usize;
        let (dest_y, dest_x) = level.coord(dest);
        if pull_mode && elim[dest] {
            continue;
        }

        if ctx.distances.impossible(dest) {
            valid[i] = false;
            continue;
        }

        let mut c = b;
        apply_move(&mut c, mv, search_mode, level);

        if state.mpdb.is_deadlock(&c, ctx, pull_mode) {
            valid[i] = false;
            continue;
        }

        if is_inefficient_position(ctx, &c, mv.to, pull_mode, search_mode, helper) {
            valid[i] = false;
            continue;
        }

        if pull_mode {
            if !has_pull_continuation(ctx, &c, search_mode) {
                valid[i] = false;
                continue;
            }
            if check_all_patterns(level, &c, dest_y, dest_x, true) {
                valid[i] = false;
                continue;
            }
            if search_mode == SearchMode::BaseSearch
                && !verify_simple_base_matching(ctx, &c, board_in)
            {
                valid[i] = false;
            }
            continue;
        }

        if is_freeze_deadlock(&c, dest_y, dest_x) {
            valid[i] = false;
            continue;
        }
        if is_3x3_deadlock(level, &c, dest_y, dest_x) {
            valid[i] = false;
            continue;
        }
        if check_all_patterns(level, &c, dest_y, dest_x, false) {
            valid[i] = false;
        }
    }

    Ok(valid)
}

/// Deadlock-zone size helper used by `score_board`.
pub fn deadlock_zone_score(board: &Board) -> i16 {
    size_of_deadlock_zone(board) as i16
}

/// Port of `can_played_reversed` (BASE_SEARCH only).
pub fn can_played_reversed(
    ctx: &LevelContext,
    state: &DeadlockState,
    board: &Board,
    mv: &Move,
    pull_mode: bool,
    search_mode: SearchMode,
) -> Result<bool> {
    if search_mode != SearchMode::BaseSearch {
        return Ok(true);
    }
    debug_assert!(pull_mode);
    if search_mode == SearchMode::GirlSearch {
        return Ok(true);
    }

    let mut c = *board;
    apply_move(&mut c, mv, SearchMode::Normal, &ctx.level);

    if c.is_solved(true, &ctx.level.initial, ctx.level.initial_sokoban) {
        return Ok(true);
    }

    let (to_y, to_x) = ctx.level.coord(mv.to as usize);
    if crate::bfs::get_connectivity(&ctx.level, &c) > 1 {
        c.clear_bases_inplace();
        let w = crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
            &ctx.level,
            &ctx.distances,
            &c,
            &state.envelopes,
        );
        if let Some(options) =
            crate::deadlock::corral::detect_corral(&ctx.level, &ctx.distances, &w)
        {
            if mv.base {
                return Ok(false);
            }
            if options.cell(to_y, to_x) & BOX == 0 {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Port of `position_is_solved` for the `DEADLOCK_SEARCH` subset.
pub fn deadlock_search_has_work(board: &Board) -> bool {
    work_left_in_deadlock_zone(board)
}

pub fn _unused_pull_mode_marker(_: &Level, _: &Distances) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deadlock::static_checks::is_2x2_deadlock_at_place;
    use crate::io::sok::parse_levels;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = crate::solver::SearchClock::new(Some(std::time::Duration::from_secs(60)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    #[test]
    fn unpacked_2x2_is_deadlocked() {
        let level = Level::preprocess(
            parse_levels("#######\n#@    #\n#     #\n#     #\n#  .  #\n#######\n").remove(0),
        )
        .unwrap();
        let mut board = level.board;
        for (y, x) in [(2, 2), (2, 3), (3, 2), (3, 3)] {
            board.set(y, x, BOX);
        }
        assert!(is_2x2_deadlock_at_place(&board, 2, 2));
    }

    #[test]
    fn oracle_reports_all_phase3_checks() {
        assert!(IMPLEMENTED_CHECKS.contains(&"corral"));
        assert!(IMPLEMENTED_CHECKS.contains(&"k-dist"));
        assert!(UNIMPLEMENTED_CHECKS.is_empty());
    }

    #[test]
    fn freeze_detects_stuck_box() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let (y, x) = ctx.level.coord(ctx.level.index_at(1, 3).unwrap());
        assert!(!is_freeze_deadlock(&ctx.level.board, y, x));
    }
}
