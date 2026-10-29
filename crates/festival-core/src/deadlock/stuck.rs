//! Learned stuck patterns (`stuck.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::IS_DEADLOCK;
use crate::deadlock::mini::can_put_all_on_targets;
use crate::deadlock::state::DeadlockState;
use crate::deadlock::wobblers::remove_wobblers;
use crate::deadlock::xy::{clear_xy_boxes, xy_deadlock_search};
use crate::error::Result;
use crate::search::tree::{ExpansionId, NONE_EXPANSION, Tree};
use crate::solver::LevelContext;

const MAX_STUCK_PATTERN_SIZE: usize = 12;
const MAX_STUCK_PATTERNS: usize = 500;

pub struct StuckPattern {
    boxes: Vec<usize>,
    board: Board,
    pull: bool,
}

#[derive(Default)]
pub struct StuckPatterns {
    entries: Vec<StuckPattern>,
}

impl StuckPatterns {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn is_in_stuck_patterns(
    ctx: &LevelContext,
    state: &DeadlockState,
    board: &Board,
    pull_mode: bool,
) -> bool {
    if state.stuck.entries.is_empty() {
        return false;
    }

    let mut indices = [false; MAX_INNER];
    for (i, slot) in indices.iter_mut().enumerate().take(ctx.level.index_num()) {
        let (y, x) = ctx.level.coord(i);
        *slot = board.cell(y, x) & BOX != 0;
    }

    for pattern in &state.stuck.entries {
        if pattern.pull != pull_mode {
            continue;
        }
        if !pattern.boxes.iter().all(|&index| indices[index]) {
            continue;
        }
        let Some((y, x)) = board.sokoban_position() else {
            continue;
        };
        if pattern.board.cell(y, x) & SOKOBAN != 0 {
            return true;
        }
    }
    false
}

pub fn tree_board_matches_patterns(
    ctx: &LevelContext,
    state: &DeadlockState,
    board: &Board,
    pull_mode: bool,
) -> bool {
    is_in_stuck_patterns(ctx, state, board, pull_mode)
}

fn add_stuck_pattern(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    pull_mode: bool,
) -> Option<()> {
    if is_in_stuck_patterns(ctx, state, board, pull_mode) {
        return None;
    }
    let n = board.boxes_in_level();
    if n == 0 || n > MAX_STUCK_PATTERN_SIZE {
        return None;
    }
    if state.stuck.entries.len() >= MAX_STUCK_PATTERNS - 1 {
        return None;
    }

    let mut boxes = Vec::with_capacity(n);
    for i in 0..ctx.level.index_num() {
        let (y, x) = ctx.level.coord(i);
        if board.cell(y, x) & BOX != 0 {
            boxes.push(i);
        }
    }
    state.stuck.entries.push(StuckPattern {
        boxes,
        board: *board,
        pull: pull_mode,
    });
    Some(())
}

#[derive(Clone, Copy)]
enum DistillMethod {
    OnTargets,
    Xy,
}

fn distill_pattern(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &mut Board,
    pull_mode: bool,
    method: DistillMethod,
) -> Result<()> {
    'restart: loop {
        for y in 0..board.height() {
            for x in 0..board.width() {
                if board.cell(y, x) & BOX == 0 {
                    continue;
                }
                let mut c = *board;
                c.and_not(y, x, BOX);
                c.expand_sokoban_cloud();
                if c.all_boxes_in_place(pull_mode, &ctx.level.initial) {
                    continue;
                }

                let deadlocked = match method {
                    DistillMethod::OnTargets => {
                        if pull_mode {
                            return Ok(());
                        }
                        can_put_all_on_targets(ctx, state, &c, 1000)? == 0
                    }
                    DistillMethod::Xy => {
                        xy_deadlock_search(ctx, state, &c, pull_mode)? == IS_DEADLOCK
                    }
                };

                if deadlocked {
                    *board = c;
                    continue 'restart;
                }
            }
        }
        return Ok(());
    }
}

pub fn register_mini_corral(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
) -> Result<()> {
    let mut b = ctx.level.initial;
    b.clear_boxes_inplace();
    b.clear_sokoban_inplace();

    if is_in_stuck_patterns(ctx, state, &b, false) {
        return Ok(());
    }

    for y in 0..b.height() {
        for x in 0..b.width() {
            if board_in.cell(y, x) & BOX != 0 {
                b.or(y, x, BOX);
            }
            if board_in.cell(y, x) == WALL && ctx.level.initial.cell(y, x) != WALL {
                b.or(y, x, BOX);
            }
        }
    }

    let Some((y, x)) = board_in.sokoban_position() else {
        return Ok(());
    };
    b.or(y, x, SOKOBAN);
    b.expand_sokoban_cloud();

    if can_put_all_on_targets(ctx, state, &b, 1000)? != 0 {
        return Ok(());
    }

    distill_pattern(ctx, state, &mut b, false, DistillMethod::OnTargets)?;
    if b.boxes_in_level() > MAX_STUCK_PATTERN_SIZE {
        return Ok(());
    }
    let _ = add_stuck_pattern(ctx, state, &b, false);
    Ok(())
}

pub fn slow_xy_deadlock(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    pull_mode: bool,
) -> Result<()> {
    let mut b = *board_in;
    if is_in_stuck_patterns(ctx, state, &b, pull_mode) {
        return Ok(());
    }

    if !pull_mode {
        remove_wobblers(&ctx.level, &mut b, pull_mode);
    }
    clear_xy_boxes(ctx, state, &mut b, pull_mode)?;

    let n = b.boxes_in_level();
    if n == 0 || n > MAX_STUCK_PATTERN_SIZE || n == ctx.level.box_count {
        return Ok(());
    }

    if xy_deadlock_search(ctx, state, &b, pull_mode)? == IS_DEADLOCK {
        distill_pattern(ctx, state, &mut b, pull_mode, DistillMethod::Xy)?;
        let _ = add_stuck_pattern(ctx, state, &b, pull_mode);
    }
    Ok(())
}

pub fn is_stuck_deadlock(
    ctx: &LevelContext,
    state: &DeadlockState,
    board: &Board,
    pull_mode: bool,
) -> bool {
    is_in_stuck_patterns(ctx, state, board, pull_mode)
}

pub fn learn_from_subtrees(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    tree: &Tree,
    mut expansion: ExpansionId,
) -> Result<()> {
    let pull_mode = tree.pull_mode;
    while expansion != NONE_EXPANSION {
        let e = &tree.expansions[expansion as usize];
        if e.subtree_size == 64 + 64 * pull_mode as u32 {
            let board = tree.expansion_board(expansion)?;
            slow_xy_deadlock(ctx, state, &board, pull_mode)?;
        }
        expansion = e.father;
    }
    Ok(())
}

pub fn recheck_tree_with_patterns(
    ctx: &LevelContext,
    state: &DeadlockState,
    tree: &mut Tree,
) -> Result<()> {
    for i in 0..tree.expansions.len() {
        let (node, deadlocked) = {
            let e = &tree.expansions[i];
            (e.node, tree.nodes[e.node as usize].deadlocked)
        };
        if deadlocked {
            continue;
        }
        let board = tree.expansion_board(i as u32)?;
        if is_in_stuck_patterns(ctx, state, &board, tree.pull_mode) {
            tree.set_subtree_as_deadlocked(ctx, i as u32)?;
        }
        let _ = node;
    }
    Ok(())
}
