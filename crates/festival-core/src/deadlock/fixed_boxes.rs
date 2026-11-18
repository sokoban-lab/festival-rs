//! Fixed-box analysis (`fixed_boxes.cpp`).
//!
//! These checks turn boxes that can never move again into walls, which shrinks
//! the move generator's search space. The C++ routine iterates until a fixed
//! point is reached.

use crate::analysis::envelope::EnvelopePatterns;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::diags::{DiagSpan, collect_diags, diag_has_no_targets, wallify_diag};
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

fn fix_crystaline_with_scratch(board: &mut Board, final_board: &mut Board) -> bool {
    final_board.copy_without_sokoban_from(board);
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
        let row = i * MAX_SIZE;
        for j in 0..final_board.width() {
            if (final_board.cells()[row + j] & board.cells()[row + j]) & BOX != 0 {
                board.set(i, j, WALL);
                fixed_box = true;
            }
        }
    }

    fixed_box
}

/// Retrograde analysis: boxes that cannot be pulled away from the final
/// position are turned into walls in `board`.
pub fn fix_crystaline(board: &mut Board) -> bool {
    let mut final_board = Board::new(board.width(), board.height()).expect("same dims");
    fix_crystaline_with_scratch(board, &mut final_board)
}

fn turn_immovable_boxes_to_walls_with_scratch(
    level: &Level,
    distances: &Distances,
    board: &mut Board,
    remaining: &mut Board,
) -> bool {
    remaining.copy_active_from(board);
    remove_all_pushable_boxes(level, distances, remaining);

    let mut changed = false;
    for i in 0..board.height() {
        let row = i * MAX_SIZE;
        for j in 0..board.width() {
            if remaining.cells()[row + j] & BOX != 0 {
                board.set(i, j, WALL);
                changed = true;
            }
        }
    }
    changed
}

/// Removes all pushable boxes and turns the remaining ones into walls.
pub fn turn_immovable_boxes_to_walls(
    level: &Level,
    distances: &Distances,
    board: &mut Board,
) -> bool {
    let mut remaining = Board::new(board.width(), board.height()).expect("same dims");
    turn_immovable_boxes_to_walls_with_scratch(level, distances, board, &mut remaining)
}

pub fn fix_diagonals(board: &mut Board) -> bool {
    let mut spans = [DiagSpan::default(); MAX_INNER];
    let count = collect_diags(board, &mut spans);
    if count == 0 {
        return false;
    }

    let mut changed = false;
    for span in &spans[..count] {
        let i = span.y as usize;
        let j = span.x as usize;
        let n = span.len as usize;
        if diag_has_no_targets(board, i, j, n, span.right) {
            changed |= wallify_diag(board, i, j, n, span.right);
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
    let mut walls = Board::new(board.width(), board.height()).expect("same dims");
    walls.copy_active_from(board);
    envelopes.fix_boxes_using_envelopes(&mut walls);

    let mut scratch = Board::new(board.width(), board.height()).expect("same dims");
    loop {
        let mut changed = turn_2x2_to_walls(&mut walls);
        changed |= close_holes(&mut walls);
        changed |=
            turn_immovable_boxes_to_walls_with_scratch(level, distances, &mut walls, &mut scratch);
        changed |= fix_diagonals(&mut walls);
        changed |= fix_crystaline_with_scratch(&mut walls, &mut scratch);
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

#[inline]
fn is_occupied_after_move(
    board: &Board,
    y: isize,
    x: isize,
    from_y: isize,
    from_x: isize,
    to_y: isize,
    to_x: isize,
) -> bool {
    if y == to_y && x == to_x {
        return true;
    }
    let mut cell = board.get(y, x);
    if y == from_y && x == from_x {
        cell &= !BOX;
    }
    cell & OCCUPIED != 0
}

pub fn is_box_2x2_frozen_after_move(level: &Level, board: &Board, m: &Move) -> bool {
    let (from_y, from_x) = level.coord(m.from as usize);
    let (to_y, to_x) = level.coord(m.to as usize);
    debug_assert!(board.cell(from_y, from_x) & BOX != 0, "missing box");

    let (fy, fx) = (from_y as isize, from_x as isize);
    let (ty, tx) = (to_y as isize, to_x as isize);
    for a in ty - 1..=ty {
        for b in tx - 1..=tx {
            if is_occupied_after_move(board, a, b, fy, fx, ty, tx)
                && is_occupied_after_move(board, a, b + 1, fy, fx, ty, tx)
                && is_occupied_after_move(board, a + 1, b, fy, fx, ty, tx)
                && is_occupied_after_move(board, a + 1, b + 1, fy, fx, ty, tx)
            {
                return true;
            }
        }
    }
    false
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
    fn frozen_after_move_matches_board_copy() {
        let lvl = level("#######\n#@    #\n#     #\n#     #\n#  .  #\n#######\n");
        let mut b = lvl.board;
        for (y, x) in [(2, 2), (2, 3), (3, 2), (3, 4)] {
            b.set(y, x, BOX);
        }
        let from_ok = lvl.index_at(3, 4).unwrap() as u16;
        let to_frozen = lvl.index_at(3, 3).unwrap() as u16;
        let to_open = lvl.index_at(2, 4).unwrap() as u16;
        let mv_frozen = Move {
            from: from_ok,
            to: to_frozen,
            ..Move::default()
        };
        let mv_open = Move {
            from: from_ok,
            to: to_open,
            ..Move::default()
        };
        assert!(is_box_2x2_frozen_after_move(&lvl, &b, &mv_frozen));
        assert!(!is_box_2x2_frozen_after_move(&lvl, &b, &mv_open));

        // Exhaustively compare against reference board copy on every valid (from, to) pair.
        for from in 0..lvl.index_num() {
            let (fy, fx) = lvl.coord(from);
            if b.cell(fy, fx) & BOX == 0 {
                continue;
            }
            for to in 0..lvl.index_num() {
                if to == from {
                    continue;
                }
                let (ty, tx) = lvl.coord(to);
                if b.cell(ty, tx) & OCCUPIED != 0 {
                    continue;
                }
                let mv = Move {
                    from: from as u16,
                    to: to as u16,
                    ..Move::default()
                };
                let mut moved = b;
                moved.and_not(fy, fx, BOX);
                moved.or(ty, tx, BOX);
                let expected = is_box_2x2_frozen_at_index(&lvl, &moved, to);
                assert_eq!(is_box_2x2_frozen_after_move(&lvl, &b, &mv), expected);
            }
        }
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
