// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Fixed-box analysis (`fixed_boxes.cpp`).
//!
//! These checks turn boxes that can never move again into walls, which shrinks
//! the move generator's search space. The C++ routine iterates until a fixed
//! point is reached.

use crate::analysis::envelope::EnvelopePatterns;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::diags::{diag_has_no_targets, mark_diags, wallify_diag};
use crate::deadlock::direction::deadlock_in_direction;
use crate::distance::Distances;
use crate::level::Level;
use crate::moves::Move;

/// Turns every full 2x2 square into walls. One pass may not catch boxes that
/// become blocked by walls created later in the same pass.
pub fn turn_2x2_to_walls(board: &mut Board) -> bool {
    let mut changed = false;
    for i in 0..board.height().saturating_sub(1) {
        for j in 0..board.width().saturating_sub(1) {
            let mut sum = 0;
            for y in 0..2 {
                for x in 0..2 {
                    if board.cell(i + y, j + x) & OCCUPIED != 0 {
                        sum += 1;
                    }
                }
            }
            if sum == 4 {
                for y in 0..2 {
                    for x in 0..2 {
                        if board.cell(i + y, j + x) & BOX != 0 {
                            changed = true;
                        }
                        board.set(i + y, j + x, WALL);
                    }
                }
            }
        }
    }
    changed
}

/// Closes one-cell holes with three surrounding walls.
pub fn close_holes(board: &mut Board) -> bool {
    for i in 1..board.height().saturating_sub(1) {
        for j in 1..board.width().saturating_sub(1) {
            if board.cell(i, j) == SPACE {
                //    $
                //   # #
                //    #
                for k in 0..4 {
                    let wall1 = board.get(
                        i as isize + DELTA_Y[(k + 1) & 3],
                        j as isize + DELTA_X[(k + 1) & 3],
                    ) == WALL;
                    let wall2 = board.get(
                        i as isize + DELTA_Y[(k + 2) & 3],
                        j as isize + DELTA_X[(k + 2) & 3],
                    ) == WALL;
                    let wall3 = board.get(
                        i as isize + DELTA_Y[(k + 3) & 3],
                        j as isize + DELTA_X[(k + 3) & 3],
                    ) == WALL;
                    if wall1 && wall2 && wall3 {
                        board.set(i, j, WALL);
                        return true;
                    }
                }
            }

            if board.cell(i, j) == SOKOBAN {
                //    .
                //   #.#
                //    #
                for k in 0..4 {
                    let sokoban = board
                        .get(i as isize + DELTA_Y[k & 3], j as isize + DELTA_X[k & 3])
                        & SOKOBAN
                        != 0;
                    let wall1 = board.get(
                        i as isize + DELTA_Y[(k + 1) & 3],
                        j as isize + DELTA_X[(k + 1) & 3],
                    ) == WALL;
                    let wall2 = board.get(
                        i as isize + DELTA_Y[(k + 2) & 3],
                        j as isize + DELTA_X[(k + 2) & 3],
                    ) == WALL;
                    let wall3 = board.get(
                        i as isize + DELTA_Y[(k + 3) & 3],
                        j as isize + DELTA_X[(k + 3) & 3],
                    ) == WALL;
                    if sokoban && wall1 && wall2 && wall3 {
                        board.set(i, j, WALL);
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Removes every box that can still be pushed somewhere, assuming the player
/// can stand anywhere.
pub fn remove_all_pushable_boxes(level: &Level, distances: &Distances, board: &mut Board) {
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..board.height() {
            for j in 0..board.width() {
                if board.cell(i, j) & BOX == 0 {
                    continue;
                }

                for k in 0..4 {
                    let to_y = i as isize + DELTA_Y[k];
                    let to_x = j as isize + DELTA_X[k];
                    if board.get(to_y, to_x) & OCCUPIED != 0 {
                        continue;
                    }

                    let from_y = i as isize - DELTA_Y[k];
                    let from_x = j as isize - DELTA_X[k];
                    if board.get(from_y, from_x) & OCCUPIED != 0 {
                        continue;
                    }

                    if deadlock_in_direction(level, distances, board, i, j, k) {
                        continue;
                    }

                    board.and_not(i, j, BOX);
                    changed = true;
                    break;
                }
            }
        }
    }
}

/// Retrograde analysis: boxes that cannot be pulled away from the final
/// position are turned into walls in `board`.
pub fn fix_crystaline(board: &mut Board) -> bool {
    let mut final_board = *board;
    final_board.clear_sokoban_inplace();
    final_board.go_to_final_position_with_given_walls();

    loop {
        let mut removed_box = false;
        for i in 0..final_board.height() {
            for j in 0..final_board.width() {
                if final_board.cell(i, j) != PACKED_BOX {
                    continue;
                }
                for k in 0..4 {
                    if final_board.get(i as isize + DELTA_Y[k], j as isize + DELTA_X[k]) & OCCUPIED
                        != 0
                    {
                        continue;
                    }
                    if final_board.get(i as isize + 2 * DELTA_Y[k], j as isize + 2 * DELTA_X[k])
                        & OCCUPIED
                        != 0
                    {
                        continue;
                    }

                    final_board.and_not(i, j, BOX);
                    removed_box = true;
                    break;
                }
            }
        }
        if !removed_box {
            break;
        }
    }

    let mut fixed_box = false;
    for i in 0..final_board.height() {
        for j in 0..final_board.width() {
            if final_board.cell(i, j) & BOX != 0 && board.cell(i, j) & BOX != 0 {
                fixed_box = true;
            }
        }
    }

    if fixed_box {
        for i in 0..final_board.height() {
            for j in 0..final_board.width() {
                if final_board.cell(i, j) & BOX != 0 && board.cell(i, j) & BOX != 0 {
                    board.set(i, j, WALL);
                }
            }
        }
    }

    fixed_box
}

/// Removes all pushable boxes and turns the remaining ones into walls.
pub fn turn_immovable_boxes_to_walls(
    level: &Level,
    distances: &Distances,
    board: &mut Board,
) -> bool {
    let mut remaining = *board;
    remove_all_pushable_boxes(level, distances, &mut remaining);

    let mut changed = false;
    for i in 0..board.height() {
        for j in 0..board.width() {
            if remaining.cell(i, j) & BOX != 0 {
                board.set(i, j, WALL);
                changed = true;
            }
        }
    }
    changed
}

pub fn fix_diagonals(board: &mut Board) -> bool {
    let mut changed = false;
    let (diag_right, diag_left) = mark_diags(board);

    for i in 0..board.height() {
        for j in 0..board.width() {
            let n = diag_right.cell(i, j);
            if n != 0 && diag_has_no_targets(board, i, j, n as usize, true) {
                changed |= wallify_diag(board, i, j, n as usize, true);
            }

            let n = diag_left.cell(i, j);
            if n != 0 && diag_has_no_targets(board, i, j, n as usize, false) {
                changed |= wallify_diag(board, i, j, n as usize, false);
            }
        }
    }
    changed
}

/// Port of `turn_fixed_boxes_into_walls`.
pub fn turn_fixed_boxes_into_walls(
    level: &Level,
    distances: &Distances,
    board: &Board,
    envelopes: &EnvelopePatterns,
) -> Board {
    let mut walls = *board;
    envelopes.fix_boxes_using_envelopes(&mut walls);

    loop {
        let mut changed = turn_2x2_to_walls(&mut walls);
        changed |= close_holes(&mut walls);
        changed |= turn_immovable_boxes_to_walls(level, distances, &mut walls);
        changed |= fix_diagonals(&mut walls);
        changed |= fix_crystaline(&mut walls);
        if !changed {
            break;
        }
    }
    walls
}

pub fn is_box_2x2_frozen_at_index(level: &Level, board: &Board, index: usize) -> bool {
    let (y, x) = level.coord(index);
    debug_assert!(board.cell(y, x) & BOX != 0, "missing box");

    for a in y as isize - 1..=y as isize {
        for b in x as isize - 1..=x as isize {
            if board.get(a, b) & OCCUPIED != 0
                && board.get(a, b + 1) & OCCUPIED != 0
                && board.get(a + 1, b) & OCCUPIED != 0
                && board.get(a + 1, b + 1) & OCCUPIED != 0
            {
                return true;
            }
        }
    }
    false
}

pub fn is_box_2x2_frozen_after_move(level: &Level, board: &Board, m: &Move) -> bool {
    let (from_y, from_x) = level.coord(m.from as usize);
    let (to_y, to_x) = level.coord(m.to as usize);
    debug_assert!(board.cell(from_y, from_x) & BOX != 0, "missing box");

    let mut moved = *board;
    moved.and_not(from_y, from_x, BOX);
    moved.or(to_y, to_x, BOX);
    is_box_2x2_frozen_at_index(level, &moved, m.to as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    fn board(text: &str) -> Board {
        parse_levels(text).remove(0).board.unwrap()
    }

    #[test]
    fn fills_full_2x2_squares() {
        let mut b = board("#####\n#$$ #\n#$$ #\n#  .#\n#####\n");
        assert!(turn_2x2_to_walls(&mut b));
        assert_eq!(b.cell(1, 1), WALL);
        assert_eq!(b.cell(2, 2), WALL);
    }

    #[test]
    fn closes_a_three_walled_hole() {
        let mut b = board("###\n# #\n###\n");
        assert!(close_holes(&mut b));
        assert_eq!(b.cell(1, 1), WALL);
    }

    #[test]
    fn immovable_corner_box_becomes_wall() {
        let lvl = level("#######\n#@ $ .#\n#######\n");
        let distances = crate::distance::compute(&lvl);
        // Preprocessing wallifies decorative boxes, so build the corner box on
        // top of a level that already has its analysis in place.
        let mut b = lvl.board;
        b.set(1, 1, BOX);
        assert!(turn_immovable_boxes_to_walls(&lvl, &distances, &mut b));
        assert_eq!(b.cell(1, 1), WALL);
    }

    #[test]
    fn crystalline_marks_a_packed_box_that_cannot_be_pulled() {
        let mut b = board("#####\n#   #\n# * #\n#  ##\n#####\n");
        assert!(fix_crystaline(&mut b));
        assert_eq!(b.cell(2, 2), WALL);
    }

    #[test]
    fn detects_frozen_2x2_boxes() {
        let lvl = level("#######\n#@    #\n#     #\n#  .  #\n#######\n");
        let mut b = lvl.board;
        for (y, x) in [(1, 2), (1, 3), (2, 2), (2, 3)] {
            b.set(y, x, BOX);
        }
        let index = lvl.index_at(1, 2).unwrap();
        assert!(is_box_2x2_frozen_at_index(&lvl, &b, index));
    }

    #[test]
    fn fixed_point_conversion_leaves_open_floor() {
        let lvl = level("#######\n#@ $ .#\n#######\n");
        let distances = crate::distance::compute(&lvl);
        let envelopes = EnvelopePatterns::new();
        let walls = turn_fixed_boxes_into_walls(&lvl, &distances, &lvl.board, &envelopes);
        assert_eq!(walls.cell(1, 4) & OCCUPIED, 0);
    }
}
