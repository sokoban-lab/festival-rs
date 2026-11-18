//! Diagonal deadlock detection (`diags.cpp`).

use crate::board::Board;
use crate::constants::*;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DiagSpan {
    pub y: u8,
    pub x: u8,
    pub len: u8,
    pub right: bool,
}

#[inline]
fn inner_get(inner: &[u8; MAX_INNER], width: usize, height: usize, y: isize, x: isize) -> u8 {
    if y < 0 || x < 0 {
        return 0;
    }
    let (yu, xu) = (y as usize, x as usize);
    if yu >= height || xu >= width {
        return 0;
    }
    inner[yu * MAX_SIZE + xu]
}

fn mark_inner_cavities(board: &Board, inner: &mut [u8; MAX_INNER]) -> bool {
    let height = board.height();
    let width = board.width();
    if height < 3 || width < 3 {
        return false;
    }
    let cells = board.cells();
    let mut found = false;
    for y in 1..height - 1 {
        let row = y * MAX_SIZE;
        let up_row = (y - 1) * MAX_SIZE;
        let dn_row = (y + 1) * MAX_SIZE;
        for x in 1..width - 1 {
            if cells[row + x] & (OCCUPIED | SOKOBAN) != 0 {
                continue;
            }
            if cells[up_row + x] & OCCUPIED == 0
                || cells[row + x + 1] & OCCUPIED == 0
                || cells[dn_row + x] & OCCUPIED == 0
                || cells[row + x - 1] & OCCUPIED == 0
            {
                continue;
            }
            if !found {
                inner[..height * MAX_SIZE].fill(0);
                found = true;
            }
            inner[row + x] = 1;
        }
    }
    found
}

#[inline]
fn check_right_diag_at(
    board: &Board,
    inner: &[u8; MAX_INNER],
    width: usize,
    height: usize,
    y: usize,
    x: usize,
) -> Option<usize> {
    let start = board.get(y as isize - 1, x as isize - 1) & OCCUPIED != 0
        || (board.get(y as isize - 1, x as isize) == WALL
            && board.get(y as isize, x as isize - 1) == WALL);
    if !start {
        return None;
    }

    let mut k = 1usize;
    while inner_get(inner, width, height, (y + k) as isize, (x + k) as isize) != 0 {
        k += 1;
    }

    let end = board.get((y + k) as isize, (x + k) as isize) & OCCUPIED != 0
        || (board.get((y + k - 1) as isize, (x + k) as isize) == WALL
            && board.get((y + k) as isize, (x + k - 1) as isize) == WALL);
    end.then_some(k)
}

#[inline]
fn check_left_diag_at(
    board: &Board,
    inner: &[u8; MAX_INNER],
    width: usize,
    height: usize,
    y: usize,
    x: usize,
) -> Option<usize> {
    let start = board.get(y as isize - 1, x as isize + 1) & OCCUPIED != 0
        || (board.get(y as isize - 1, x as isize) == WALL
            && board.get(y as isize, x as isize + 1) == WALL);
    if !start {
        return None;
    }

    let mut k = 1usize;
    while inner_get(
        inner,
        width,
        height,
        (y + k) as isize,
        x as isize - k as isize,
    ) != 0
    {
        k += 1;
    }
    let left = x as isize - k as isize;

    let end = board.get((y + k) as isize, left) & OCCUPIED != 0
        || (board.get((y + k - 1) as isize, left) == WALL
            && board.get((y + k) as isize, left + 1) == WALL);
    end.then_some(k)
}

pub(crate) fn collect_diags(board: &Board, out: &mut [DiagSpan; MAX_INNER]) -> usize {
    let mut inner = [0u8; MAX_INNER];
    if !mark_inner_cavities(board, &mut inner) {
        return 0;
    }

    let height = board.height();
    let width = board.width();
    let mut count = 0usize;

    for y in 1..height - 1 {
        let row = y * MAX_SIZE;
        for x in 1..width - 1 {
            if inner[row + x] == 0 {
                continue;
            }
            if let Some(k) = check_right_diag_at(board, &inner, width, height, y, x) {
                out[count] = DiagSpan {
                    y: y as u8,
                    x: x as u8,
                    len: k as u8,
                    right: true,
                };
                count += 1;
            }
            if let Some(k) = check_left_diag_at(board, &inner, width, height, y, x) {
                out[count] = DiagSpan {
                    y: y as u8,
                    x: x as u8,
                    len: k as u8,
                    right: false,
                };
                count += 1;
            }
        }
    }
    count
}

pub fn mark_diags(board: &Board) -> (Board, Board) {
    let mut diag_right = Board::new(board.width(), board.height()).expect("same dims");
    let mut diag_left = Board::new(board.width(), board.height()).expect("same dims");
    let mut spans = [DiagSpan::default(); MAX_INNER];
    let count = collect_diags(board, &mut spans);
    for span in &spans[..count] {
        if span.right {
            diag_right.set(span.y as usize, span.x as usize, span.len);
        } else {
            diag_left.set(span.y as usize, span.x as usize, span.len);
        }
    }
    (diag_right, diag_left)
}

#[inline]
fn set_wall_track(board: &mut Board, y: isize, x: isize, changed: &mut bool) {
    if y >= 0 && x >= 0 {
        let (yu, xu) = (y as usize, x as usize);
        if board.in_bounds(yu, xu) && board.cell(yu, xu) != WALL {
            board.set(yu, xu, WALL);
            *changed = true;
        }
    }
}

pub fn wallify_diag(board: &mut Board, y: usize, x: usize, k: usize, diag_right: bool) -> bool {
    let mut changed = false;
    if diag_right {
        for n in 0..k {
            set_wall_track(
                board,
                (y + n) as isize,
                x as isize + n as isize - 1,
                &mut changed,
            );
            set_wall_track(
                board,
                (y + n) as isize,
                x as isize + n as isize + 1,
                &mut changed,
            );
        }
        if board.get(y as isize - 1, x as isize - 1) & OCCUPIED != 0 {
            set_wall_track(board, y as isize - 1, x as isize - 1, &mut changed);
        }
        set_wall_track(board, y as isize - 1, x as isize, &mut changed);
        if board.get((y + k) as isize, (x + k) as isize) & OCCUPIED != 0 {
            set_wall_track(board, (y + k) as isize, (x + k) as isize, &mut changed);
        }
        set_wall_track(board, (y + k) as isize, (x + k) as isize - 1, &mut changed);
    } else {
        for n in 0..k {
            set_wall_track(
                board,
                (y + n) as isize,
                x as isize - n as isize - 1,
                &mut changed,
            );
            set_wall_track(
                board,
                (y + n) as isize,
                x as isize - n as isize + 1,
                &mut changed,
            );
        }
        if board.get(y as isize - 1, x as isize + 1) & OCCUPIED != 0 {
            set_wall_track(board, y as isize - 1, x as isize + 1, &mut changed);
        }
        set_wall_track(board, y as isize - 1, x as isize, &mut changed);
        if board.get((y + k) as isize, x as isize - k as isize) & OCCUPIED != 0 {
            set_wall_track(
                board,
                (y + k) as isize,
                x as isize - k as isize,
                &mut changed,
            );
        }
        set_wall_track(
            board,
            (y + k) as isize,
            x as isize - k as isize + 1,
            &mut changed,
        );
    }
    changed
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
    let mut inner = [0u8; MAX_INNER];
    if !mark_inner_cavities(board, &mut inner) {
        return false;
    }

    let height = board.height();
    let width = board.width();
    for y in 1..height - 1 {
        let row = y * MAX_SIZE;
        for x in 1..width - 1 {
            if inner[row + x] == 0 {
                continue;
            }
            if let Some(k) = check_right_diag_at(board, &inner, width, height, y, x)
                && diag_deadlock(board, y, x, k, 1)
            {
                return true;
            }
            if let Some(k) = check_left_diag_at(board, &inner, width, height, y, x)
                && diag_deadlock(board, y, x, k, -1)
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
