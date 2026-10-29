//! Deadlock-zone bookkeeping (`deadlock_utils.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::level::Level;

pub fn clear_deadlock_zone_inplace(board: &mut Board) {
    board.clear_deadlock_zone_inplace();
}

pub fn clear_deadlock_zone(board: &Board) -> Board {
    let mut out = *board;
    out.clear_deadlock_zone_inplace();
    out
}

pub fn work_left_in_deadlock_zone(board: &Board) -> bool {
    for y in 0..board.height() {
        for x in 0..board.width() {
            let cell = board.cell(y, x);
            if cell & DEADLOCK_ZONE == 0 {
                continue;
            }
            if cell & SOKOBAN != 0 {
                continue;
            }
            if cell & BOX != 0 && cell & TARGET == 0 {
                return true;
            }
            if cell & TARGET != 0 && cell & BOX == 0 {
                return true;
            }
        }
    }
    false
}

/// Only meaningful in pull mode: every box outside the zone must sit on an
/// initial box square, and every initial box must still exist.
pub fn work_left_outside_deadlock_zone(level: &Level, board: &Board) -> bool {
    let initial = &level.initial;
    for y in 0..board.height() {
        for x in 0..board.width() {
            let cell = board.cell(y, x);
            if cell & DEADLOCK_ZONE != 0 {
                continue;
            }
            if cell & BOX != 0 && initial.cell(y, x) & BOX == 0 {
                return true;
            }
            if initial.cell(y, x) & BOX != 0 && cell & BOX == 0 {
                return true;
            }
        }
    }
    false
}

pub fn get_sokoban_cloud_as_group(board: &Board) -> Board {
    let mut group = *board;
    for y in 0..group.height() {
        for x in 0..group.width() {
            let value = if board.cell(y, x) & SOKOBAN != 0 {
                1
            } else {
                0
            };
            group.set(y, x, value);
        }
    }
    group
}

fn can_be_pushed_sideways(board: &Board, y: usize, x: usize, d: usize) -> bool {
    let d = (d + 1) & 3;
    let dy = DELTA_Y[d];
    let dx = DELTA_X[d];
    let (yi, xi) = (y as isize, x as isize);
    if board.get(yi + dy, xi + dx) & OCCUPIED == 0
        && board.get(yi + dy * 2, xi + dx * 2) & OCCUPIED == 0
        && board.get(yi - dy, xi - dx) & OCCUPIED == 0
    {
        return true;
    }

    let d = (d + 2) & 3;
    let dy = DELTA_Y[d];
    let dx = DELTA_X[d];
    board.get(yi + dy, xi + dx) & OCCUPIED == 0
        && board.get(yi + dy * 2, xi + dx * 2) & OCCUPIED == 0
        && board.get(yi - dy, xi - dx) & OCCUPIED == 0
}

fn update_deadlock_zone_from_group_push_mode(level: &Level, board: &mut Board, group: &Board) {
    let mut previous = *board;
    previous.clear_deadlock_zone_inplace();
    for y in 0..board.height() {
        for x in 0..board.width() {
            let value = if board.cell(y, x) & DEADLOCK_ZONE != 0 {
                1
            } else {
                0
            };
            // C++ copies the bit into a separate board; we only need membership.
            if value == 0 {
                previous.set(y, x, 0);
            } else {
                previous.set(y, x, 1);
            }
        }
    }

    board.clear_deadlock_zone_inplace();

    for index in 0..level.index_num() {
        let (y, x) = level.coord(index);
        if group.cell(y, x) == 0 {
            continue;
        }

        board.or(y, x, DEADLOCK_ZONE);

        for k in 0..8 {
            let ny = y as isize + DELTA_Y_8[k];
            let nx = x as isize + DELTA_X_8[k];
            if board.get(ny, nx) != WALL {
                board.or(ny as usize, nx as usize, DEADLOCK_ZONE);
            }
        }

        for k in 0..4 {
            let by = y as isize + DELTA_Y[k];
            let bx = x as isize + DELTA_X[k];
            if board.get(by, bx) & BOX == 0 {
                continue;
            }
            let cy = y as isize + DELTA_Y[k] * 2;
            let cx = x as isize + DELTA_X[k] * 2;
            if board.get(cy, cx) & BOX == 0 {
                continue;
            }
            if !can_be_pushed_sideways(board, cy as usize, cx as usize, k) {
                board.or(cy as usize, cx as usize, DEADLOCK_ZONE);
            }
        }
    }

    for y in 0..board.height() {
        for x in 0..board.width() {
            if previous.cell(y, x) == 0 {
                board.and_not(y, x, DEADLOCK_ZONE);
            }
        }
    }
}

fn add_islands(level: &Level, board: &mut Board) {
    let mut dz = Board::new(board.width(), board.height()).expect("same dimensions");
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & DEADLOCK_ZONE != 0 {
                dz.set(y, x, 1);
            }
        }
    }

    for y in 0..board.height() {
        for x in 0..board.width() {
            if !level.is_inner(y, x) {
                continue;
            }
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }
            if dz.cell(y, x) != 0 {
                continue;
            }

            let touches =
                (0..4).any(|k| dz.get(y as isize + DELTA_Y[k], x as isize + DELTA_X[k]) != 0);
            if !touches {
                continue;
            }

            if board.get(y as isize + 1, x as isize) & OCCUPIED != 0
                && board.get(y as isize - 1, x as isize) & OCCUPIED != 0
                && board.get(y as isize, x as isize + 1) & OCCUPIED != 0
                && board.get(y as isize, x as isize - 1) & OCCUPIED != 0
            {
                board.or(y, x, DEADLOCK_ZONE);
                for k in 0..8 {
                    let ny = y as isize + DELTA_Y_8[k];
                    let nx = x as isize + DELTA_X_8[k];
                    if board.get(ny, nx) & BOX != 0 {
                        board.or(ny as usize, nx as usize, DEADLOCK_ZONE);
                    }
                }
            }
        }
    }
}

pub fn init_deadlock_zone_from_group(
    level: &Level,
    board: &mut Board,
    group: &Board,
    pull_mode: bool,
) {
    if pull_mode {
        board.clear_deadlock_zone_inplace();
        for y in 0..board.height() {
            for x in 0..board.width() {
                if group.cell(y, x) == 0 {
                    continue;
                }
                board.or(y, x, DEADLOCK_ZONE);
                for k in 0..8 {
                    let ny = y as isize + DELTA_Y_8[k];
                    let nx = x as isize + DELTA_X_8[k];
                    if board.get(ny, nx) & BOX != 0 {
                        board.or(ny as usize, nx as usize, DEADLOCK_ZONE);
                    }
                }
            }
        }
        add_islands(level, board);
        return;
    }

    for y in 0..board.height() {
        for x in 0..board.width() {
            board.or(y, x, DEADLOCK_ZONE);
        }
    }
    update_deadlock_zone_from_group_push_mode(level, board, group);
}

pub fn set_deadlock_zone_in_pull_mode(level: &Level, board: &mut Board) {
    let group = get_sokoban_cloud_as_group(board);
    init_deadlock_zone_from_group(level, board, &group, true);
}

pub fn update_deadlock_zone_push_mode(level: &Level, board: &mut Board) {
    let mut unreachable = Board::new(board.width(), board.height()).expect("same dimensions");
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & SOKOBAN != 0 {
                continue;
            }
            if !level.is_inner(y, x) {
                continue;
            }
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }
            if board.cell(y, x) & DEADLOCK_ZONE == 0 {
                continue;
            }
            unreachable.set(y, x, 1);
        }
    }
    update_deadlock_zone_from_group_push_mode(level, board, &unreachable);
}

pub fn near_deadlock_zone(board: &Board, y: usize, x: usize) -> bool {
    (0..8).any(|i| {
        board.get(y as isize + DELTA_Y_8[i], x as isize + DELTA_X_8[i]) & DEADLOCK_ZONE != 0
    })
}

pub fn clear_frozen_boxes_away_from_deadlock_zone(level: &Level, board: &mut Board) {
    let mut has_frozen = false;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if !level.is_inner(y, x) {
                continue;
            }
            if board.cell(y, x) == WALL && level.initial.cell(y, x) != WALL {
                has_frozen = true;
            }
        }
    }
    if !has_frozen {
        return;
    }

    for y in 0..board.height() {
        for x in 0..board.width() {
            if !level.is_inner(y, x) {
                continue;
            }
            if board.cell(y, x) != WALL || level.initial.cell(y, x) == WALL {
                continue;
            }
            if !near_deadlock_zone(board, y, x) {
                board.set(y, x, level.initial.cell(y, x) & TARGET);
            }
        }
    }
    board.expand_sokoban_cloud();
}

pub fn clear_boxes_out_of_deadlock_zone(board: &mut Board) {
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & DEADLOCK_ZONE == 0 {
                board.and_not(y, x, BOX);
            }
        }
    }
    board.expand_sokoban_cloud();
}

pub fn exited_deadlock_zone(board: &Board) -> bool {
    for y in 0..board.height() {
        for x in 0..board.width() {
            let cell = board.cell(y, x);
            if cell & SOKOBAN != 0 && cell & DEADLOCK_ZONE == 0 {
                return true;
            }
        }
    }
    false
}

pub fn update_deadlock_zone(level: &Level, board: &mut Board, pull_mode: bool) {
    if pull_mode {
        set_deadlock_zone_in_pull_mode(level, board);
    } else {
        update_deadlock_zone_push_mode(level, board);
    }
    clear_boxes_out_of_deadlock_zone(board);
}

pub fn size_of_deadlock_zone(board: &Board) -> usize {
    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & DEADLOCK_ZONE != 0 {
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

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn push_zone_is_subset_of_previous_zone() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let mut board = level.board;
        board.clear_deadlock_zone_inplace();
        board.or(1, 2, DEADLOCK_ZONE);
        let group = Board::new(board.width(), board.height()).unwrap();
        update_deadlock_zone_push_mode(&level, &mut board);
        assert_eq!(board.cell(1, 2) & DEADLOCK_ZONE, 0);
        assert_eq!(size_of_deadlock_zone(&board), 0);
        let _ = group;
    }

    #[test]
    fn pull_zone_contains_the_player_cloud() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let mut board = level.board;
        set_deadlock_zone_in_pull_mode(&level, &mut board);
        assert!(size_of_deadlock_zone(&board) >= board.sokoban_cloud_size());
    }
}
