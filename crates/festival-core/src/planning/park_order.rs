// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Parking order extraction and scoring (`park_order.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::planning::ParkOrder;
use crate::planning::overlap;
use crate::search::helper::Helper;
use crate::search::tree::{ExpansionId, NONE_EXPANSION, Tree};
use crate::solver::LevelContext;

const FROM_INIT: i32 = -2;
const FROM_UNKNOWN: i32 = -1;
const UNTIL_NONE: i32 = -1;

pub fn verify_parking_order(ctx: &LevelContext, helper: &Helper) -> Result<()> {
    let index_num = ctx.level.index_num() as i32;
    for (i, step) in helper.parking_order.iter().enumerate() {
        if step.to < 0 || step.to >= index_num {
            return Err(SolveError::Internal(format!(
                "parking order {i} (to) is {}",
                step.to
            )));
        }
        if step.from >= index_num {
            return Err(SolveError::Internal(format!(
                "parking order {i} (from) is {}",
                step.from
            )));
        }
    }
    Ok(())
}

fn move_mandatory_boxes_to_beginning(order: &mut Vec<ParkOrder>) {
    let mut reordered = Vec::with_capacity(order.len());
    for step in order.iter() {
        if step.from == FROM_INIT && step.until == UNTIL_NONE {
            reordered.push(*step);
        }
    }
    for step in order.iter() {
        if step.from != FROM_INIT || step.until != UNTIL_NONE {
            reordered.push(*step);
        }
    }
    *order = reordered;
}

pub(crate) fn recompute_until(order: &mut [ParkOrder]) {
    for step in order.iter_mut() {
        step.until = UNTIL_NONE;
    }

    let n = order.len();
    for i in 0..n {
        for j in (i + 1)..n {
            if order[j].from == order[i].to {
                order[i].until = j as i32;
                break;
            }
        }
    }
}

fn fix_consecutive_pushes(helper: &mut Helper) -> bool {
    let order = &mut helper.parking_order;
    let mut fixed = false;
    let mut n = order.len();
    let mut i = 0usize;

    while i + 1 < n {
        if order[i].from == FROM_INIT {
            i += 1;
            continue;
        }

        // a->b b->a can be concatenated into a->a
        if order[i].to == order[i].from {
            order.remove(i);
            n -= 1;
            i = 0;
            fixed = true;
        }

        // C++ continues in the same iteration after the removal above.
        if i + 1 < n && order[i].to == order[i + 1].from {
            order[i].to = order[i + 1].to;
            order.remove(i + 1);
            n -= 1;
            i = 0;
            fixed = true;
        }

        i += 1;
    }

    recompute_until(order);
    fixed
}

/// Builds `helper.parking_order` from the path to `expansion`
/// (`get_parking_plan`).
pub fn get_parking_plan(
    ctx: &LevelContext,
    tree: &Tree,
    mut expansion: ExpansionId,
    helper: &mut Helper,
) -> Result<()> {
    let board = tree.expansion_board(expansion)?;
    let mut order: Vec<ParkOrder> = Vec::with_capacity(ctx.level.box_count + 16);

    // Initial boxes are recorded from the bottom-right, matching C++.
    for y in (0..board.height()).rev() {
        for x in (0..board.width()).rev() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            let index = ctx
                .level
                .index_at(y, x)
                .ok_or_else(|| SolveError::Internal("initial box outside inner".into()))?;
            order.push(ParkOrder {
                from: FROM_INIT,
                to: index as i32,
                until: UNTIL_NONE,
            });
        }
    }

    while tree.expansions[expansion as usize].father != NONE_EXPANSION {
        let mv = tree.get_move_to_node(expansion)?;
        let (from, to) = if !mv.base {
            (mv.from as i32, mv.to as i32)
        } else {
            (mv.from as i32, -1)
        };
        expansion = tree.expansions[expansion as usize].father;

        if from == to {
            continue;
        }

        order.push(ParkOrder {
            from: to,
            to: from,
            until: UNTIL_NONE,
        });
        if order.len() >= MAX_SOL_LEN {
            return Err(SolveError::Internal("plan is too long".into()));
        }
    }

    helper.parking_order = order;
    recompute_until(&mut helper.parking_order);
    fix_consecutive_pushes(helper);
    move_mandatory_boxes_to_beginning(&mut helper.parking_order);
    Ok(())
}

/// When all boxes stay on the board, keep only the final permutation
/// (`keep_only_perm`).
pub fn keep_only_perm(helper: &mut Helper) {
    let mut kept = Vec::with_capacity(helper.parking_order.len());
    for mut step in helper.parking_order.iter().copied() {
        if step.until != UNTIL_NONE {
            continue;
        }
        if step.from != FROM_INIT {
            step.from = FROM_UNKNOWN;
        }
        kept.push(step);
    }
    helper.parking_order = kept;
}

/// Port of `reduce_parking_order`.
pub fn reduce_parking_order(ctx: &LevelContext, helper: &mut Helper) -> Result<()> {
    overlap::save_full_parking_order(ctx, helper)?;

    let init_num = helper
        .parking_order
        .iter()
        .filter(|step| step.from == FROM_INIT)
        .count();
    helper.boxes_were_removed = init_num != ctx.level.box_count;

    if !helper.boxes_were_removed {
        keep_only_perm(helper);
    }
    Ok(())
}

/// Port of `conclude_parking`.
pub fn conclude_parking(
    ctx: &LevelContext,
    tree: &Tree,
    best: Option<ExpansionId>,
    helper: &mut Helper,
) -> Result<bool> {
    let selected = if tree.search_mode == crate::constants::SearchMode::GirlSearch {
        tree.find_best_rl_parking_node(&ctx.girl)
    } else {
        best
    };
    let Some(expansion) = selected else {
        helper.parking_order.clear();
        return Ok(false);
    };

    let depth = tree.expansions[expansion as usize].depth as usize;
    if depth + ctx.level.box_count + 2 >= MAX_SOL_LEN {
        helper.parking_order.clear();
        return Ok(false);
    }

    get_parking_plan(ctx, tree, expansion, helper)?;
    Ok(true)
}

/// Port of `score_parked_boxes`.
pub fn score_parked_boxes(ctx: &LevelContext, board: &Board, helper: &Helper) -> i32 {
    let count = helper.parking_order.len().min(MAX_SOL_LEN);
    let plan = &helper.parking_order[..count];
    let mut has_box = [false; MAX_SOL_LEN];
    for (i, step) in plan.iter().enumerate() {
        if step.to < 0 {
            continue;
        }
        let (y, x) = ctx.level.coord(step.to as usize);
        has_box[i] = board.cell(y, x) & BOX != 0;
    }
    let has_box = &has_box[..count];

    let init_num = plan.iter().filter(|step| step.from == FROM_INIT).count();

    if mandatory_inits_missing(has_box, plan, init_num) {
        return get_number_of_mandatory_inits(has_box, plan, init_num);
    }

    let mut packed = plan.len() as i32;

    for i in (init_num..plan.len()).rev() {
        if has_box[i] {
            continue;
        }
        let until = plan[i].until;
        if until == UNTIL_NONE || until >= packed {
            packed = i as i32;
        }
    }

    let mut good_init = 0i32;
    for (i, present) in has_box.iter().take(init_num).enumerate() {
        if *present || plan[i].until < packed {
            good_init += 1;
        }
    }

    packed - (init_num as i32 - good_init)
}

fn mandatory_inits_missing(has_box: &[bool], plan: &[ParkOrder], init_num: usize) -> bool {
    (0..init_num).any(|i| plan[i].until == UNTIL_NONE && !has_box[i])
}

fn get_number_of_mandatory_inits(has_box: &[bool], plan: &[ParkOrder], init_num: usize) -> i32 {
    (0..init_num)
        .filter(|&i| plan[i].until == UNTIL_NONE && has_box[i])
        .count() as i32
}

fn all_fixed_initials_on_place(ctx: &LevelContext, board: &Board, helper: &Helper) -> bool {
    for step in helper.parking_order.iter() {
        if step.from != FROM_INIT || step.until != UNTIL_NONE {
            continue;
        }
        if step.to < 0 {
            return false;
        }
        let (y, x) = ctx.level.coord(step.to as usize);
        if board.cell(y, x) & BOX == 0 {
            return false;
        }
    }
    true
}

/// Port of `next_parking_place`; returns the next target cell index.
pub fn next_parking_place(ctx: &LevelContext, board: &Board, helper: &Helper) -> Option<usize> {
    if !all_fixed_initials_on_place(ctx, board, helper) {
        return None;
    }

    let has_box: Vec<bool> = helper
        .parking_order
        .iter()
        .map(|step| {
            if step.to < 0 {
                return false;
            }
            let (y, x) = ctx.level.coord(step.to as usize);
            board.cell(y, x) & BOX != 0
        })
        .collect();

    let mut init_num = 0usize;
    while init_num < helper.parking_order.len() && helper.parking_order[init_num].from == FROM_INIT
    {
        init_num += 1;
    }

    let n = helper.parking_order.len();
    let mut packed = n;
    for i in (init_num..n).rev() {
        if has_box[i] {
            continue;
        }
        let until = helper.parking_order[i].until;
        if until == UNTIL_NONE || until as usize >= packed {
            packed = i;
        }
    }

    if packed == n {
        return None;
    }
    let to = helper.parking_order[packed].to;
    if to < 0 {
        return None;
    }
    Some(to as usize)
}

/// Debug helper used by tests: the order as `(from, to, until)` triples.
pub fn parking_triples(helper: &Helper) -> Vec<(i32, i32, i32)> {
    helper
        .parking_order
        .iter()
        .map(|step| (step.from, step.to, step.until))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;
    use std::time::Duration;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = crate::solver::SearchClock::new(Some(Duration::from_secs(60)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    #[test]
    fn consecutive_pushes_are_compressed() {
        let mut helper = Helper {
            parking_order: vec![
                ParkOrder {
                    from: -2,
                    to: 1,
                    until: -1,
                },
                ParkOrder {
                    from: 5,
                    to: 6,
                    until: -1,
                },
                ParkOrder {
                    from: 6,
                    to: 7,
                    until: -1,
                },
            ],
            ..Helper::default()
        };
        let fixed = fix_consecutive_pushes(&mut helper);
        assert!(fixed);
        assert_eq!(parking_triples(&helper).len(), 2);
        assert_eq!(helper.parking_order[1].to, 7);
    }

    #[test]
    fn parked_score_ignores_moved_initials() {
        let ctx = context("#######\n#@ $ .#\n# $  .#\n#  .  #\n#######\n");
        let initial = ctx.level.index_at(1, 5).unwrap() as i32;
        let fixed_init = ctx.level.index_at(2, 5).unwrap() as i32;
        let last = ctx.level.index_at(3, 4).unwrap() as i32;
        let helper = Helper {
            parking_order: vec![
                ParkOrder {
                    from: -2,
                    to: initial,
                    until: 1,
                },
                ParkOrder {
                    from: -2,
                    to: fixed_init,
                    until: -1,
                },
                ParkOrder {
                    from: -1,
                    to: last,
                    until: -1,
                },
            ],
            ..Helper::default()
        };
        let mut board = ctx.level.initial;
        board.and_not(1, 3, BOX);
        board.and_not(2, 2, BOX);
        board.or(2, 5, BOX);
        board.or(3, 4, BOX);
        // The moved init is absent, but only `until == -1` inits are mandatory.
        assert_eq!(score_parked_boxes(&ctx, &board, &helper), 3);
    }
}
