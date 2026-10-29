//! Imagined goal boards (`imagine.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::planning::ImagineData;
use crate::planning::ParkOrder;
use crate::planning::park_order;
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

/// Builds the imagined boards for a full parking order (`init_imagine`).
pub fn build(ctx: &LevelContext, full: &[ParkOrder]) -> Result<ImagineData> {
    if full.len() >= MAX_SOL_LEN {
        return Err(SolveError::Internal("plan is too long".into()));
    }

    let mut data = ImagineData::default();

    // A box that came from a sink (removed from the board) makes the plan
    // unusable for imagined scoring.
    if full.iter().any(|step| step.from == -1) {
        return Ok(data);
    }

    if full.iter().any(|step| step.from == step.to) {
        return Err(SolveError::Internal("shifting a box in place".into()));
    }

    let mut board = ctx.level.initial;
    board.clear_boxes_inplace();
    board.clear_sokoban_inplace();
    let mut fixed = Board::new(board.width(), board.height())?;
    let mut permanent = 0usize;

    for step in full {
        let (to_y, to_x) = ctx.level.coord(step.to as usize);
        if step.until == -1 {
            permanent += 1;
            fixed.set(to_y, to_x, 1);
        }

        if step.until == -1 && step.from != -2 {
            data.imagined_boards.push(board);
            data.fixed_at_step.push(fixed);
            data.imagine_packed_num.push(permanent);
        }

        board.or(to_y, to_x, BOX);
        if step.from >= 0 {
            let (from_y, from_x) = ctx.level.coord(step.from as usize);
            board.and_not(from_y, from_x, BOX);
        }
    }

    if data.imagined_boards.is_empty() {
        // The rare plan where every box is already in place.
        if full.len() != ctx.level.box_count {
            return Err(SolveError::Internal("corrupted plan".into()));
        }
        data.imagined_boards.push(board);
        data.fixed_at_step.push(fixed);
        data.imagine_packed_num.push(permanent);
    }

    Ok(data)
}

/// Port of `get_imagined_board`; returns the board and its index.
pub fn get_imagined_board(
    ctx: &LevelContext,
    board: &Board,
    helper: &Helper,
) -> Result<(Board, usize)> {
    let imagine = helper
        .imagine
        .as_ref()
        .ok_or_else(|| SolveError::Internal("imagine not allocated".into()))?;
    if imagine.imagined_boards.is_empty() {
        return Err(SolveError::Internal("no imagined boards".into()));
    }

    let packed = park_order::score_parked_boxes(ctx, board, helper);
    if packed == ctx.level.box_count as i32 {
        let relevant = imagine.imagined_boards.len() - 1;
        return Ok((*board, relevant));
    }

    let mut relevant = 0usize;
    for (i, &num) in imagine.imagine_packed_num.iter().enumerate() {
        if num as i32 > packed {
            relevant = i;
            break;
        }
        relevant = i;
    }
    Ok((imagine.imagined_boards[relevant], relevant))
}

/// Port of `imagine_score`.
pub fn imagine_score(
    ctx: &LevelContext,
    board: &Board,
    search_mode: SearchMode,
    helper: &Helper,
) -> i32 {
    if matches!(
        search_mode,
        SearchMode::ForwardWithBases | SearchMode::GirlSearch
    ) || helper.boxes_were_removed
    {
        return 0;
    }
    if search_mode == SearchMode::HfSearch {
        return compare_to_imagined_hf(board, helper);
    }
    if search_mode != SearchMode::Normal {
        return 0;
    }
    if helper.parking_order.len() != ctx.level.box_count {
        return 0;
    }

    let Ok((imagined, relevant)) = get_imagined_board(ctx, board, helper) else {
        return 0;
    };
    let Some(imagine) = helper.imagine.as_ref() else {
        return 0;
    };
    let Some(fixed) = imagine.fixed_at_step.get(relevant) else {
        return 0;
    };

    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX != 0
                && imagined.cell(y, x) & BOX != 0
                && fixed.cell(y, x) == 0
            {
                sum += 1;
            }
        }
    }
    sum
}

/// Port of `choose_imagine_advisor`.
pub fn choose_imagine_advisor(
    base_score: &ScoreElement,
    scores: &[ScoreElement],
    search_mode: SearchMode,
) -> i32 {
    if matches!(
        search_mode,
        SearchMode::ForwardWithBases | SearchMode::GirlSearch
    ) {
        return -1;
    }

    let mut best_son = -1i32;
    let mut min_dist = 0i32;
    for (i, score) in scores.iter().enumerate() {
        if score.packed_boxes < base_score.packed_boxes {
            continue;
        }
        if score.imagine <= base_score.imagine {
            continue;
        }
        if best_son == -1 || score.dist_to_imagined < min_dist {
            min_dist = score.dist_to_imagined;
            best_son = i as i32;
        }
    }
    best_son
}

/// Port of `set_imagined_hf_board`.
pub fn set_imagined_hf_board(board: &Board, helper: &mut Helper) {
    let mut imagined = *board;
    for y in 0..imagined.height() {
        for x in 0..imagined.width() {
            imagined.and_not(y, x, TARGET);
            if imagined.cell(y, x) & BOX != 0 {
                imagined.or(y, x, BASE);
            }
        }
    }
    helper.imagined_hf_board = imagined;
}

/// Port of `compare_to_imagined_hf`.
pub fn compare_to_imagined_hf(board: &Board, helper: &Helper) -> i32 {
    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX != 0 && helper.imagined_hf_board.cell(y, x) & BOX != 0 {
                sum += 1;
            }
        }
    }
    sum
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
    fn imagined_board_places_boxes_on_plan_targets() {
        let ctx = context("#########\n#@ $ . .#\n#########\n");
        let first = ctx.level.index_at(1, 5).unwrap() as i32;
        let second = ctx.level.index_at(1, 7).unwrap() as i32;
        let full = vec![
            ParkOrder {
                from: -2,
                to: first,
                until: 1,
            },
            ParkOrder {
                from: first,
                to: second,
                until: -1,
            },
        ];
        let data = build(&ctx, &full).unwrap();
        assert_eq!(data.imagined_boards.len(), 1);
        // The snapshot is taken before the permanent box is placed.
        assert_eq!(data.imagined_boards[0].cell(1, 7) & BOX, 0);
        assert_ne!(data.imagined_boards[0].cell(1, 5) & BOX, 0);
        assert_eq!(data.fixed_at_step[0].cell(1, 7), 1);
    }

    #[test]
    fn sink_plans_disable_imagined_boards() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let full = vec![ParkOrder {
            from: -1,
            to: 3,
            until: -1,
        }];
        assert!(build(&ctx, &full).unwrap().imagined_boards.is_empty());
    }
}
