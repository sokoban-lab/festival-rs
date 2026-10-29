//! Freeze checks for a single push (`deadlock.cpp:deadlock_in_direction`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::static_checks::is_2x2_deadlock_at_place;
use crate::distance::Distances;
use crate::level::Level;

/// 2x2 windows to inspect after pushing in each direction.
const SQUARE_PLACE_Y: [[isize; 2]; 4] = [[-2, -2], [-1, 0], [1, 1], [-1, 0]];
const SQUARE_PLACE_X: [[isize; 2]; 4] = [[-1, 0], [1, 1], [-1, 0], [-2, -2]];

/// Returns `true` if pushing the box at `(y, x)` in direction `d` produces a
/// frozen position (or is immediately impossible).
pub fn deadlock_in_direction(
    level: &Level,
    distances: &Distances,
    board: &Board,
    y: usize,
    x: usize,
    d: usize,
) -> bool {
    let dy = DELTA_Y[d];
    let dx = DELTA_X[d];
    let dy2 = DELTA_Y[(d + 1) & 3];
    let dx2 = DELTA_X[(d + 1) & 3];
    let new_y = y as isize + dy;
    let new_x = x as isize + dx;

    if new_y < 0 || new_x < 0 {
        return true;
    }
    let new_y = new_y as usize;
    let new_x = new_x as usize;

    if new_y >= board.height() || new_x >= board.width() || !level.is_inner(new_y, new_x) {
        return true;
    }
    let Some(new_index) = level.index_at(new_y, new_x) else {
        return true;
    };
    if distances.impossible(new_index) {
        return true;
    }

    debug_assert!(board.cell(y, x) & BOX != 0, "missing box");
    debug_assert!(
        board.get(new_y as isize, new_x as isize) & OCCUPIED == 0,
        "no space"
    );

    // Two walls behind the destination:
    //   #
    //  #$
    //   ^ = (y, x)
    if board.get(new_y as isize, new_x as isize) & TARGET == 0
        && board.get(new_y as isize + dy, new_x as isize + dx) == WALL
    {
        if board.get(new_y as isize + dy2, new_x as isize + dx2) == WALL {
            return true;
        }
        if board.get(new_y as isize - dy2, new_x as isize - dx2) == WALL {
            return true;
        }
    }

    // Patterns of type
    //   1  #$   2
    //   3   $#  4
    //        ^
    if board.get(new_y as isize + dy, new_x as isize + dx) & BOX != 0 {
        let wall4 = board.get(new_y as isize + dy2, new_x as isize + dx2) == WALL;
        let wall3 = board.get(new_y as isize - dy2, new_x as isize - dx2) == WALL;
        let wall2 = board.get(new_y as isize + dy + dy2, new_x as isize + dx + dx2) == WALL;
        let wall1 = board.get(new_y as isize + dy - dy2, new_x as isize + dx - dx2) == WALL;

        let wall1 = usize::from(wall1);
        let wall2 = usize::from(wall2);
        let wall3 = usize::from(wall3);
        let wall4 = usize::from(wall4);

        if (wall1 + wall4 == 2) || (wall2 + wall3 == 2) {
            if board.get(new_y as isize, new_x as isize) & TARGET == 0 {
                return true;
            }
            if board.get(new_y as isize + dy, new_x as isize + dx) & TARGET == 0 {
                return true;
            }
        }
    }

    let mut pushed = *board;
    pushed.and_not(y, x, BOX);
    pushed.or(new_y, new_x, BOX);

    let res_0 = is_2x2_deadlock_at_place(
        &pushed,
        y as isize + SQUARE_PLACE_Y[d][0],
        x as isize + SQUARE_PLACE_X[d][0],
    );
    let res_1 = is_2x2_deadlock_at_place(
        &pushed,
        y as isize + SQUARE_PLACE_Y[d][1],
        x as isize + SQUARE_PLACE_X[d][1],
    );

    res_0 || res_1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn setup(text: &str) -> (Level, Distances) {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let distances = crate::distance::compute(&level);
        (level, distances)
    }

    #[test]
    fn pushing_into_a_wall_is_a_deadlock() {
        let (level, distances) = setup("#######\n#@$  .#\n#######\n");
        assert!(deadlock_in_direction(
            &level,
            &distances,
            &level.board,
            1,
            2,
            UP
        ));
    }

    #[test]
    fn pushing_along_an_open_corridor_is_safe() {
        let (level, distances) = setup("#######\n#@$  .#\n#######\n");
        assert!(!deadlock_in_direction(
            &level,
            &distances,
            &level.board,
            1,
            2,
            RIGHT
        ));
    }

    #[test]
    fn wall_ahead_and_wall_on_one_side_deadlocks() {
        let (level, distances) = setup("#######\n#@$ # #\n#   . #\n#######\n");
        // The destination (1, 3) has a wall ahead at (1, 4) and a wall on the
        // upper side at (0, 3), which is a corner for the box.
        assert!(deadlock_in_direction(
            &level,
            &distances,
            &level.board,
            1,
            2,
            RIGHT
        ));
    }
}
