// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Backup plan and overlap score (`overlap.cpp`).

use crate::board::Board;
use crate::constants::BOX;
use crate::error::Result;
use crate::planning::imagine;
use crate::search::helper::Helper;
use crate::solver::LevelContext;

/// Copies the current plan into `full_parking_order` and builds the imagined
/// boards from it, before the plan is reduced to a permutation.
pub fn save_full_parking_order(ctx: &LevelContext, helper: &mut Helper) -> Result<()> {
    helper.full_parking_order = helper.parking_order.clone();

    helper.used_in_plan.zero();
    for step in &helper.full_parking_order {
        if step.to < 0 {
            continue;
        }
        let (y, x) = ctx.level.coord(step.to as usize);
        helper.used_in_plan.or(y, x, 1);
    }

    let full = helper.full_parking_order.clone();
    helper.imagine = Some(imagine::build(ctx, &full)?);
    Ok(())
}

/// Port of `overlap_score`; used by the GIRL planning score.
pub fn overlap_score(ctx: &LevelContext, board: &Board, helper: &Helper) -> i32 {
    if helper.boxes_were_removed || helper.full_parking_order.is_empty() {
        return 0;
    }

    let mut score = 0i32;
    let mut best = -1i32;
    for step in &helper.full_parking_order {
        if step.to >= 0 {
            let (to_y, to_x) = ctx.level.coord(step.to as usize);
            if board.cell(to_y, to_x) & BOX != 0 {
                score += 1;
            }
        }
        if step.from >= 0 {
            let (from_y, from_x) = ctx.level.coord(step.from as usize);
            if board.cell(from_y, from_x) & BOX != 0 {
                score -= 1;
            }
        }
        if score >= best {
            best = score;
        }
    }
    best
}
