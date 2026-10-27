// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Two-box pattern database (`mpdb2.cpp`).

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::mini::can_put_all_on_targets;
use crate::deadlock::state::DeadlockState;
use crate::error::Result;
use crate::solver::LevelContext;

const MAX_MPDB_PATTERNS: usize = 50;

#[derive(Default)]
pub struct Mpdb {
    push: Vec<(usize, usize)>,
    pull: Vec<(usize, usize)>,
}

impl Mpdb {
    pub fn is_deadlock(&self, board: &Board, ctx: &LevelContext, pull_mode: bool) -> bool {
        let list = if pull_mode { &self.pull } else { &self.push };
        for &(first, second) in list {
            let (y, x) = ctx.level.coord(first);
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            let (y, x) = ctx.level.coord(second);
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            return true;
        }
        false
    }

    pub fn push_len(&self) -> usize {
        self.push.len()
    }

    pub fn pull_len(&self) -> usize {
        self.pull.len()
    }
}

fn two_near_wall(board: &Board) -> bool {
    for i in 0..board.height().saturating_sub(1) {
        for j in 0..board.width().saturating_sub(1) {
            let mut boxes = 0;
            let mut walls = 0;
            for a in 0..2 {
                for b in 0..2 {
                    let cell = board.cell(i + a, j + b);
                    if cell == WALL {
                        walls += 1;
                    }
                    if cell & BOX != 0 {
                        boxes += 1;
                    }
                }
            }
            if boxes == 2 && walls == 2 {
                return true;
            }
        }
    }
    false
}

fn check_known_deadlock(ctx: &LevelContext, board: &Board) -> bool {
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            if crate::deadlock::oracle::check_all_patterns(&ctx.level, board, y, x, false) {
                return true;
            }
        }
    }
    false
}

fn very_pullable(board: &Board, y: usize, x: usize) -> bool {
    let mut sum = 0;
    for i in 0..4 {
        if board.get(y as isize + DELTA_Y[i], x as isize + DELTA_X[i]) & OCCUPIED == 0
            && board.get(y as isize + DELTA_Y[i] * 2, x as isize + DELTA_X[i] * 2) & OCCUPIED == 0
        {
            sum += 1;
        }
    }
    sum == 4
}

pub fn build_mpdb2(ctx: &LevelContext, state: &mut DeadlockState) -> Result<()> {
    let level = &ctx.level;
    state.mpdb = Mpdb::default();
    if level.box_count == 1 {
        return Ok(());
    }

    let mut empty = level.initial;
    empty.clear_boxes_inplace();
    empty.clear_sokoban_inplace();

    for i in 0..level.index_num() {
        if ctx.distances.impossible(i) {
            continue;
        }
        let (yi, xi) = level.coord(i);
        if level.forbidden_push.cell(yi, xi) != 0 {
            continue;
        }
        for j in (i + 1)..level.index_num() {
            if ctx.distances.impossible(j) {
                continue;
            }
            let (yj, xj) = level.coord(j);
            if level.forbidden_push.cell(yj, xj) != 0 {
                continue;
            }
            if !ctx.hotspots.double_blocking(i, j) {
                continue;
            }

            let mut b = empty;
            b.or(yi, xi, BOX);
            b.or(yj, xj, BOX);

            if b.all_boxes_in_place(false, &level.initial) {
                continue;
            }
            if bfs::get_connectivity(level, &b) > 1 {
                continue;
            }
            if two_near_wall(&b) {
                continue;
            }
            if check_known_deadlock(ctx, &b) {
                continue;
            }

            b.put_sokoban_everywhere();
            if can_put_all_on_targets(ctx, state, &b, 100)? != 0 {
                continue;
            }

            if state.mpdb.push.len() < MAX_MPDB_PATTERNS {
                state.mpdb.push.push((i, j));
            }
        }
    }
    Ok(())
}

pub fn build_pull_mpdb2(ctx: &LevelContext, state: &mut DeadlockState) -> Result<()> {
    let level = &ctx.level;
    state.mpdb.pull = Vec::new();
    if level.box_count == 1 {
        return Ok(());
    }

    let mut empty = level.initial;
    empty.clear_boxes_inplace();
    empty.clear_sokoban_inplace();

    for i in 0..level.index_num() {
        if ctx.distances.impossible(i) {
            continue;
        }
        let (y1, x1) = level.coord(i);
        if level.forbidden_pull.cell(y1, x1) != 0 {
            continue;
        }
        for j in (i + 1)..level.index_num() {
            if ctx.distances.impossible(j) {
                continue;
            }
            let (y2, x2) = level.coord(j);
            if level.forbidden_pull.cell(y2, x2) != 0 {
                continue;
            }
            if y1 != y2 && x1 != x2 {
                continue;
            }
            let distance = y1.abs_diff(y2) + x1.abs_diff(x2);
            if distance == 1 || distance >= 10 {
                continue;
            }
            if ctx.distances.bfs_from_to(i, j) as usize != distance {
                continue;
            }

            let mut b = empty;
            b.or(y1, x1, BOX);
            b.or(y2, x2, BOX);
            if very_pullable(&b, y1, x1) && very_pullable(&b, y2, x2) {
                continue;
            }
            if b.all_boxes_in_place(true, &level.initial) {
                continue;
            }
            if bfs::get_connectivity(level, &b) > 1 {
                continue;
            }

            b.put_sokoban_everywhere();
            if !crate::deadlock::xy::is_xy_deadlock(ctx, state, &b, true)? {
                continue;
            }

            if state.mpdb.pull.len() < MAX_MPDB_PATTERNS {
                state.mpdb.pull.push((i, j));
            }
        }
    }

    Ok(())
}
