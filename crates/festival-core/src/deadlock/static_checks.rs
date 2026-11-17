use crate::board::Board;
use crate::constants::*;

/// Port of `mark_deadlock_2x2`.
///
/// `blocked` must have the same dimensions as `board` and is fully overwritten.
pub fn mark_deadlock_2x2(board: &Board, blocked: &mut Board) {
    let height = board.height();
    let width = board.width();
    let cells = board.cells();
    blocked.zero();

    for i in 0..height.saturating_sub(1) {
        let r0 = i * MAX_SIZE;
        let r1 = (i + 1) * MAX_SIZE;
        for j in 0..width.saturating_sub(1) {
            // The C++ routine clears only the sokoban; boxes count as occupied.
            let c00 = cells[r0 + j] & !SOKOBAN;
            let c01 = cells[r0 + j + 1] & !SOKOBAN;
            let c10 = cells[r1 + j] & !SOKOBAN;
            let c11 = cells[r1 + j + 1] & !SOKOBAN;

            let sum = u32::from(c00 & OCCUPIED != 0)
                + u32::from(c01 & OCCUPIED != 0)
                + u32::from(c10 & OCCUPIED != 0)
                + u32::from(c11 & OCCUPIED != 0);
            let stuck = u32::from(c00 != WALL && c00 & TARGET == 0)
                + u32::from(c01 != WALL && c01 & TARGET == 0)
                + u32::from(c10 != WALL && c10 & TARGET == 0)
                + u32::from(c11 != WALL && c11 & TARGET == 0);

            if sum == 3 && stuck != 0 {
                blocked.set(i, j, 1);
                blocked.set(i, j + 1, 1);
                blocked.set(i + 1, j, 1);
                blocked.set(i + 1, j + 1, 1);
            }

            if c00 == WALL && c11 == WALL {
                if c01 == 0 {
                    blocked.set(i, j + 1, 1);
                }
                if c10 == 0 {
                    blocked.set(i + 1, j, 1);
                }
            }

            if c01 == WALL && c10 == WALL {
                if c00 == 0 {
                    blocked.set(i, j, 1);
                }
                if c11 == 0 {
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
