//! Packing feasibility mini search (`mini_search.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::positions::PositionSet;
use crate::deadlock::state::DeadlockState;
use crate::error::Result;
use crate::search::helper::Helper;
use crate::solver::LevelContext;

fn get_move_with_most_packing(positions: &PositionSet) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut max_packed = -1i16;
    let mut best_connectivity = i16::MAX;

    for i in 0..positions.len() {
        let p = &positions.positions[i];
        if p.position_deadlocked {
            continue;
        }
        for j in 0..p.moves.len() {
            if p.move_deadlocked[j] || p.place_in_positions[j].is_some() {
                continue;
            }
            let score = p.scores[j];
            if score.boxes_on_targets < max_packed {
                continue;
            }
            if score.boxes_on_targets > max_packed {
                max_packed = score.boxes_on_targets;
                best_connectivity = score.connectivity;
                best = Some((i, j));
                continue;
            }
            if score.connectivity < best_connectivity {
                best_connectivity = score.connectivity;
                best = Some((i, j));
            }
        }
    }
    best
}

fn has_a_move_with_all_boxes_packed(positions: &PositionSet, p: usize) -> bool {
    let position = &positions.positions[p];
    let boxes = position.board.boxes_in_level() as i16;
    position
        .scores
        .iter()
        .any(|score| score.boxes_on_targets == boxes)
}

/// `can_put_all_on_targets`: `1` solved, `0` deadlocked, `-1` search aborted.
pub fn can_put_all_on_targets(
    ctx: &LevelContext,
    deadlock: &mut DeadlockState,
    board: &Board,
    max_size: usize,
) -> Result<i32> {
    let mut helper = Helper::default();
    let mut positions = PositionSet::new(max_size);
    positions.set_first_position(
        ctx,
        board,
        false,
        SearchMode::MiniSearch,
        &mut helper,
        deadlock,
    )?;
    let mut pos_num = positions.len();

    loop {
        let last = pos_num - 1;
        if positions.positions[last].moves.len() > 1000 {
            return Ok(-1);
        }
        if has_a_move_with_all_boxes_packed(&positions, last) {
            return Ok(1);
        }

        let Some((pos, son)) = get_move_with_most_packing(&positions) else {
            return Ok(0);
        };

        positions.expand_position(ctx, pos, son, false, &mut helper, deadlock)?;
        pos_num = positions.len();

        let last = pos_num - 1;
        if positions.positions[last]
            .board
            .all_boxes_in_place(false, &ctx.level.initial)
        {
            return Ok(1);
        }

        if pos_num == max_size {
            return Ok(-1);
        }
    }
}
