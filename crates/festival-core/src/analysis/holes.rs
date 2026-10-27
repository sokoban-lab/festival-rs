// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Target-hole detection (`holes.cpp`).

use crate::board::Board;
use crate::constants::*;

fn is_square_a_target_hole(board: &Board, y: usize, x: usize) -> bool {
    if board.cell(y, x) & TARGET == 0 {
        return false;
    }
    board.count_walls_around(y, x) == 3
}

pub fn mark_target_holes(board: &Board) -> (Board, Board) {
    let mut b = board.without_boxes();
    b.clear_sokoban_inplace();
    let mut target_holes = Board::new(board.width(), board.height()).expect("same dims");

    let mut changed = true;
    while changed {
        changed = false;
        for y in 0..b.height() {
            for x in 0..b.width() {
                if is_square_a_target_hole(&b, y, x) {
                    b.set(y, x, WALL);
                    target_holes.set(y, x, 1);
                    changed = true;
                }
            }
        }
    }

    let mut semi_holes = Board::new(board.width(), board.height()).expect("same dims");
    for y in 0..b.height() {
        for x in 0..b.width() {
            if b.cell(y, x) & TARGET == 0 {
                continue;
            }
            for k in 0..4 {
                let val1 = b.at_dir(y, x, k);
                let val2 = b.at_dir(y, x, (k + 1) & 3);
                if val1 == WALL && val2 == WALL {
                    semi_holes.set(y, x, 1);
                }
            }
        }
    }

    (target_holes, semi_holes)
}

pub fn turn_target_holes_into_walls(board: &mut Board, target_holes: &Board) {
    for y in 0..board.height() {
        for x in 0..board.width() {
            if target_holes.cell(y, x) != 0 {
                board.set(y, x, WALL);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;

    #[test]
    fn detects_three_sided_target() {
        let level =
            Level::preprocess(parse_levels("#####\n###.#\n#@$ #\n#   #\n#####\n").remove(0))
                .unwrap();
        let (holes, _) = mark_target_holes(&level.board);
        assert_eq!(holes.cell(1, 3), 1);
    }
}
