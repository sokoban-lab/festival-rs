// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Static deadlock oracle used by the search during Phase 2.
//!
//! Only checks that are statically sound are enabled: embedded patterns,
//! 2x2 blocks, freeze detection and 3x3 blocks. Corral, matching, MPDB2,
//! k-distance, wobblers and learned patterns arrive with Phase 3 and are
//! deliberately not approximated here.

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::patterns::{check_deadlock_pattern, patterns};
use crate::deadlock::static_checks::is_2x2_deadlock_at_place;
use crate::distance::Distances;
use crate::level::Level;
use crate::moves::{Move, apply_move};

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
        return patterns
            .normal
            .iter()
            .any(|pattern| check_deadlock_pattern(pattern, &b, box_y, box_x));
    }

    let (sok_y, sok_x) = level.initial_sokoban;
    if b.get(sok_y as isize, sok_x as isize) & SOKOBAN != 0 {
        return false;
    }

    patterns
        .pull
        .iter()
        .any(|pattern| check_deadlock_pattern(pattern, &b, box_y, box_x))
}

fn has_pull_continuation(level: &Level, board: &Board) -> bool {
    if board.all_boxes_in_place(true, &level.initial) {
        let (sok_y, sok_x) = level.initial_sokoban;
        if board.get(sok_y as isize, sok_x as isize) & SOKOBAN != 0 {
            return true;
        }
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

/// Marks moves that lead to a statically dead position.
pub fn check_deadlock_in_moves(
    level: &Level,
    distances: &Distances,
    board: &Board,
    moves: &[Move],
    pull_mode: bool,
) -> Vec<bool> {
    let mut valid = vec![true; moves.len()];

    for (index, mv) in moves.iter().enumerate() {
        if mv.kill {
            continue;
        }

        if distances.impossible(mv.to as usize) {
            valid[index] = false;
            continue;
        }

        let mut c = *board;
        apply_move(&mut c, mv, SearchMode::Normal, level);
        let (dest_y, dest_x) = level.coord(mv.to as usize);

        if pull_mode {
            if !has_pull_continuation(level, &c) {
                valid[index] = false;
                continue;
            }
            if check_all_patterns(level, &c, dest_y, dest_x, true) {
                valid[index] = false;
            }
            continue;
        }

        if is_freeze_deadlock(&c, dest_y, dest_x) {
            valid[index] = false;
            continue;
        }
        if is_3x3_deadlock(level, &c, dest_y, dest_x) {
            valid[index] = false;
            continue;
        }
        if check_all_patterns(level, &c, dest_y, dest_x, false) {
            valid[index] = false;
        }
    }

    valid
}

/// Conservatively checks a position after a played move.
pub fn move_is_deadlocked(level: &Level, board: &Board, mv: &Move, pull_mode: bool) -> bool {
    if mv.kill || mv.base {
        return false;
    }

    let mut c = *board;
    apply_move(&mut c, mv, SearchMode::Normal, level);
    let (dest_y, dest_x) = level.coord(mv.to as usize);

    // Any full 2x2 block containing an unpacked box can never be resolved.
    for dy in -1isize..=0 {
        for dx in -1isize..=0 {
            if is_2x2_deadlock_at_place(&c, dest_y as isize + dy, dest_x as isize + dx) {
                return true;
            }
        }
    }

    if !pull_mode {
        if is_freeze_deadlock(&c, dest_y, dest_x) {
            return true;
        }
        if is_3x3_deadlock(level, &c, dest_y, dest_x) {
            return true;
        }
        if check_all_patterns(level, &c, dest_y, dest_x, false) {
            return true;
        }
    } else if check_all_patterns(level, &c, dest_y, dest_x, true) {
        return true;
    }

    false
}

/// Phase 2 board check: static patterns and 2x2 blocks only.
pub fn board_is_deadlocked(level: &Level, board: &Board, pull_mode: bool) -> bool {
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            if is_2x2_deadlock_at_place(board, y as isize - 1, x as isize - 1)
                || is_2x2_deadlock_at_place(board, y as isize, x as isize)
            {
                return true;
            }
            if check_all_patterns(level, board, y, x, pull_mode) {
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

    #[test]
    fn unpacked_2x2_is_deadlocked() {
        let level = level("#######\n#@    #\n#     #\n#     #\n#  .  #\n#######\n");
        let mut board = level.board;
        for (y, x) in [(2, 2), (2, 3), (3, 2), (3, 3)] {
            board.set(y, x, BOX);
        }
        assert!(is_2x2_deadlock_at_place(&board, 2, 2));
    }

    #[test]
    fn freeze_detects_stuck_box() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let (y, x) = level.coord(level.index_at(1, 3).unwrap());
        assert!(!is_freeze_deadlock(&level.board, y, x));
    }
}
