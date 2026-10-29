//! Opener/explorer advisors (`opener.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::moves::{Move, apply_move};
use crate::planning::park_order;
use crate::search::helper::Helper;
use crate::search::score::{ScoreElement, is_better_score};
use crate::solver::LevelContext;

fn are_new_moves(b: &Board, c: &Board, pull_mode: bool) -> bool {
    for i in 0..b.height() {
        for j in 0..b.width() {
            if b.cell(i, j) & BOX == 0 || c.cell(i, j) & BOX == 0 {
                continue;
            }

            for k in 0..4 {
                let sok_y = i as isize + DELTA_Y[k];
                let sok_x = j as isize + DELTA_X[k];

                let val = b.get(sok_y, sok_x);
                if val & SOKOBAN != 0 || val & BOX != 0 {
                    continue;
                }
                if c.get(sok_y, sok_x) & SOKOBAN == 0 {
                    continue;
                }

                if !pull_mode {
                    if c.get(i as isize - DELTA_Y[k], j as isize - DELTA_X[k]) & OCCUPIED == 0 {
                        return true;
                    }
                } else if c.get(sok_y + DELTA_Y[k], sok_x + DELTA_X[k]) & OCCUPIED == 0 {
                    return true;
                }
            }
        }
    }
    false
}

/// Port of `explorer`.
pub fn explorer(
    ctx: &LevelContext,
    board: &Board,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    pull_mode: bool,
) -> i32 {
    let mut best_move = -1i32;
    let mut best_score = ScoreElement::default();

    for (i, mv) in moves.iter().enumerate() {
        if scores[i].packed_boxes < base_score.packed_boxes {
            continue;
        }
        if !pull_mode && ctx.distances.from_to(mv.to as usize, mv.from as usize) == INF {
            continue;
        }

        let mut c = *board;
        apply_move(&mut c, mv, SearchMode::Normal, &ctx.level);

        if !are_new_moves(board, &c, pull_mode) {
            continue;
        }

        if best_move == -1 {
            best_score = scores[i];
            best_move = i as i32;
            continue;
        }

        if is_better_score(&scores[i], &best_score, pull_mode, SearchMode::Normal) {
            best_score = scores[i];
            best_move = i as i32;
        }
    }

    best_move
}

/// Port of `pick_opener`; the next parking place guides the box there.
pub fn pick_opener(
    ctx: &LevelContext,
    board: &Board,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    _pull_mode: bool,
    helper: &Helper,
) -> Result<i32> {
    if scores
        .iter()
        .any(|score| score.packed_boxes > base_score.packed_boxes)
    {
        return Ok(-1);
    }

    let Some(blocker_index) = park_order::next_parking_place(ctx, board, helper) else {
        return Ok(-1);
    };
    let (blocker_y, blocker_x) = ctx.level.coord(blocker_index);

    if board.cell(blocker_y, blocker_x) & BOX != 0 {
        return Err(SolveError::Internal("box in next parking place".into()));
    }

    let connectivity = base_score.connectivity;
    let rooms_score = base_score.rooms_score;
    let packed_boxes = base_score.packed_boxes;

    let mut min_dist_from = 1_000_000i32;
    let mut max_dist_to = 0i32;
    let mut best_move = -1i32;

    for (i, mv) in moves.iter().enumerate() {
        if scores[i].packed_boxes < packed_boxes {
            continue;
        }
        if scores[i].connectivity > connectivity {
            continue;
        }
        if scores[i].rooms_score > rooms_score {
            continue;
        }

        let dist1 = ctx.distances.bfs_from_to(blocker_index, mv.from as usize) as i32;
        let dist2 = ctx.distances.bfs_from_to(blocker_index, mv.to as usize) as i32;
        if dist2 < dist1 {
            continue;
        }
        if dist1 > min_dist_from {
            continue;
        }

        if dist1 < min_dist_from {
            min_dist_from = dist1;
            best_move = i as i32;
            max_dist_to = dist2;
        } else if dist1 == min_dist_from && dist2 > max_dist_to {
            max_dist_to = dist2;
            best_move = i as i32;
        }
    }

    Ok(best_move)
}
