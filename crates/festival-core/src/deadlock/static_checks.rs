// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

use crate::board::Board;
use crate::constants::*;

/// Port of `mark_deadlock_2x2`.
///
/// `blocked` must have the same dimensions as `board` and is fully overwritten.
pub fn mark_deadlock_2x2(board: &Board, blocked: &mut Board) {
    let height = board.height();
    let width = board.width();
    // The C++ routine clears only the sokoban; boxes count as occupied.
    let mut c = *board;
    c.clear_sokoban_inplace();
    blocked.zero();

    for i in 0..height.saturating_sub(1) {
        for j in 0..width.saturating_sub(1) {
            let mut sum = 0;
            let mut stuck = 0;
            for y in 0..2 {
                for x in 0..2 {
                    let val = c.cell(i + y, j + x);
                    if val & OCCUPIED != 0 {
                        sum += 1;
                    }
                    if val != WALL && val & TARGET == 0 {
                        stuck += 1;
                    }
                }
            }

            if sum == 3 && stuck != 0 {
                for y in 0..2 {
                    for x in 0..2 {
                        blocked.set(i + y, j + x, 1);
                    }
                }
            }

            if c.cell(i, j) == WALL && c.cell(i + 1, j + 1) == WALL {
                if c.cell(i, j + 1) == 0 {
                    blocked.set(i, j + 1, 1);
                }
                if c.cell(i + 1, j) == 0 {
                    blocked.set(i + 1, j, 1);
                }
            }

            if c.cell(i, j + 1) == WALL && c.cell(i + 1, j) == WALL {
                if c.cell(i, j) == 0 {
                    blocked.set(i, j, 1);
                }
                if c.cell(i + 1, j + 1) == 0 {
                    blocked.set(i + 1, j + 1, 1);
                }
            }
        }
    }
}

/// Port of `is_2x2_deadlock_at_place`: a fully occupied 2x2 square whose boxes
/// are not all packed.
pub fn is_2x2_deadlock_at_place(board: &Board, i: isize, j: isize) -> bool {
    for y in 0..2 {
        for x in 0..2 {
            if board.get(i + y, j + x) & OCCUPIED == 0 {
                return false;
            }
        }
    }

    for y in 0..2 {
        for x in 0..2 {
            let cell = board.get(i + y, j + x);
            if cell & BOX != 0 && cell & TARGET == 0 {
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
    use crate::level::Level;

    #[test]
    fn detects_three_sided_corner() {
        let level = Level::preprocess(parse_levels("#####\n#  .#\n# ##\n#@$#\n#####\n").remove(0))
            .expect("level");
        let mut blocked = Board::new(level.board.width(), level.board.height()).unwrap();
        mark_deadlock_2x2(&level.board, &mut blocked);
        assert!(blocked.cells().iter().any(|v| *v != 0));
    }
}
