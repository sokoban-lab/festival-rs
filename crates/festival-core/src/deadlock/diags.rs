//! Diagonal deadlock detection (`diags.cpp`).

use crate::board::Board;
use crate::constants::*;

fn same_board(a: &Board, b: &Board) -> bool {
    if a.width() != b.width() || a.height() != b.height() {
        return false;
    }
    for y in 0..a.height() {
        for x in 0..a.width() {
            if a.cell(y, x) != b.cell(y, x) {
                return false;
            }
        }
    }
    true
}

pub fn mark_diags(board: &Board) -> (Board, Board) {
    let mut inner = Board::new(board.width(), board.height()).expect("same dims");
    let mut diag_right = Board::new(board.width(), board.height()).expect("same dims");
    let mut diag_left = Board::new(board.width(), board.height()).expect("same dims");

    for y in 1..board.height().saturating_sub(1) {
        for x in 1..board.width().saturating_sub(1) {
            if board.cell(y, x) & OCCUPIED != 0 || board.cell(y, x) & SOKOBAN != 0 {
                continue;
            }
            if board.at_dir(y, x, 0) & OCCUPIED == 0
                || board.at_dir(y, x, 1) & OCCUPIED == 0
                || board.at_dir(y, x, 2) & OCCUPIED == 0
                || board.at_dir(y, x, 3) & OCCUPIED == 0
            {
                continue;
            }
            inner.set(y, x, 1);
        }
    }

    for y in 1..board.height().saturating_sub(1) {
        for x in 1..board.width().saturating_sub(1) {
            if inner.cell(y, x) == 0 {
                continue;
            }
            let mut start = false;
            if board.get(y as isize - 1, x as isize - 1) & OCCUPIED != 0 {
                start = true;
            }
            if board.get(y as isize - 1, x as isize) == WALL
                && board.get(y as isize, x as isize - 1) == WALL
            {
                start = true;
            }
            if !start {
                continue;
            }

            let mut k = 1;
            while inner.get((y + k) as isize, (x + k) as isize) != 0 {
                k += 1;
            }

            let mut end = false;
            if board.get((y + k) as isize, (x + k) as isize) & OCCUPIED != 0 {
                end = true;
            }
            if board.get((y + k - 1) as isize, (x + k) as isize) == WALL
                && board.get((y + k) as isize, (x + k - 1) as isize) == WALL
            {
                end = true;
            }
            if !end {
                continue;
            }
            diag_right.set(y, x, k as u8);
        }
    }

    for y in 1..board.height().saturating_sub(1) {
        for x in 1..board.width().saturating_sub(1) {
            if inner.cell(y, x) == 0 {
                continue;
            }
            let mut start = false;
            if board.get(y as isize - 1, x as isize + 1) & OCCUPIED != 0 {
                start = true;
            }
            if board.get(y as isize - 1, x as isize) == WALL
                && board.get(y as isize, x as isize + 1) == WALL
            {
                start = true;
            }
            if !start {
                continue;
            }

            let mut k = 1;
            while inner.get((y + k) as isize, (x as isize) - k as isize) != 0 {
                k += 1;
            }
            let left = x as isize - k as isize;

            let mut end = false;
            if board.get((y + k) as isize, left) & OCCUPIED != 0 {
                end = true;
            }
            if board.get((y + k - 1) as isize, left) == WALL
                && board.get((y + k) as isize, left + 1) == WALL
            {
                end = true;
            }
            if !end {
                continue;
            }
            diag_left.set(y, x, k as u8);
        }
    }

    (diag_right, diag_left)
}

fn set_wall(board: &mut Board, y: isize, x: isize) {
    if y >= 0 && x >= 0 && board.in_bounds(y as usize, x as usize) {
        board.set(y as usize, x as usize, WALL);
    }
}

fn wallify_diag_void(board: &mut Board, y: usize, x: usize, k: usize, diag_right: bool) {
    if diag_right {
        for n in 0..k {
            set_wall(board, (y + n) as isize, x as isize + n as isize - 1);
            set_wall(board, (y + n) as isize, x as isize + n as isize + 1);
        }
        if board.get(y as isize - 1, x as isize - 1) & OCCUPIED != 0 {
            set_wall(board, y as isize - 1, x as isize - 1);
        }
        set_wall(board, y as isize - 1, x as isize);
        if board.get((y + k) as isize, (x + k) as isize) & OCCUPIED != 0 {
            set_wall(board, (y + k) as isize, (x + k) as isize);
        }
        set_wall(board, (y + k) as isize, (x + k) as isize - 1);
    } else {
        for n in 0..k {
            set_wall(board, (y + n) as isize, x as isize - n as isize - 1);
            set_wall(board, (y + n) as isize, x as isize - n as isize + 1);
        }
        if board.get(y as isize - 1, x as isize + 1) & OCCUPIED != 0 {
            set_wall(board, y as isize - 1, x as isize + 1);
        }
        set_wall(board, y as isize - 1, x as isize);
        if board.get((y + k) as isize, x as isize - k as isize) & OCCUPIED != 0 {
            set_wall(board, (y + k) as isize, x as isize - k as isize);
        }
        set_wall(board, (y + k) as isize, x as isize - k as isize + 1);
    }
}

pub fn wallify_diag(board: &mut Board, y: usize, x: usize, k: usize, diag_right: bool) -> bool {
    let backup = *board;
    wallify_diag_void(board, y, x, k, diag_right);
    !same_board(board, &backup)
}

pub fn diag_has_no_targets(board: &Board, y: usize, x: usize, n: usize, diag_right: bool) -> bool {
    let delta = if diag_right { 1 } else { -1 };
    for k in 0..n {
        let yy = (y + k) as isize;
        let xx = x as isize + delta * k as isize;
        if board.get(yy, xx) & TARGET != 0 {
            return false;
        }
    }
    true
}

pub fn diag_deadlock(
    board: &Board,
    start_y: usize,
    start_x: usize,
    n: usize,
    delta: isize,
) -> bool {
    let mut good = board.get(start_y as isize - 1, start_x as isize - delta) != BOX;
    let mut last = (start_y, start_x);

    for i in 0..=n {
        let current_y = start_y as isize + i as isize;
        let current_x = start_x as isize + i as isize * delta;
        last = (current_y as usize, current_x as usize);

        let element1 = board.get(current_y - 1, current_x);
        let element2 = board.get(current_y, current_x - delta);

        if element1 == BOX || element2 == BOX || !good {
            if i == n {
                return true;
            }
            good = board.get(current_y, current_x) & TARGET != 0;
            continue;
        }

        if i == n {
            break;
        }
        if board.get(current_y, current_x) & TARGET != 0 {
            return false;
        }
    }

    board.get(last.0 as isize, last.1 as isize) == BOX
}

pub fn is_diag_deadlock(board: &Board) -> bool {
    let (diag_right, diag_left) = mark_diags(board);
    for y in 0..board.height() {
        for x in 0..board.width() {
            let n = diag_right.cell(y, x);
            if n != 0 && diag_deadlock(board, y, x, n as usize, 1) {
                return true;
            }
            let n = diag_left.cell(y, x);
            if n != 0 && diag_deadlock(board, y, x, n as usize, -1) {
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

    #[test]
    fn detects_a_right_diagonal() {
        let board = parse_levels("#######\n###   #\n## #  #\n# # # #\n#  # ##\n#   ###\n#######\n")
            .remove(0)
            .board
            .unwrap();
        let (right, _left) = mark_diags(&board);
        assert_eq!(right.cell(2, 2), 3);
    }
}
