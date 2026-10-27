// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! LURD solution replay used as the independent output validator.

use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LurdStats {
    pub moves: usize,
    pub pushes: usize,
    pub boxes_on_targets: usize,
    pub boxes_total: usize,
    pub solved: bool,
}

/// Replays a LURD string from the initial board.
///
/// U/R/D/L are player moves; a move into a box is a push. Whitespace and
/// line breaks are ignored so solutions can be wrapped.
pub fn replay(initial: &Board, lurd: &str) -> Result<LurdStats> {
    let mut board = *initial;
    let (mut py, mut px) = board.sokoban_position().ok_or(SolveError::InvalidBoard)?;

    board.clear_sokoban_inplace();
    board.or(py, px, SOKOBAN);

    let mut stats = LurdStats::default();

    for byte in lurd.bytes() {
        let dir = match byte {
            b'U' | b'u' => 0,
            b'R' | b'r' => 1,
            b'D' | b'd' => 2,
            b'L' | b'l' => 3,
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => {
                return Err(SolveError::InvalidMove(format!(
                    "unexpected LURD character {:?}",
                    byte as char
                )));
            }
        };

        let ny = py as isize + DELTA_Y[dir];
        let nx = px as isize + DELTA_X[dir];
        if ny < 0 || nx < 0 {
            return Err(SolveError::InvalidMove("move leaves the board".into()));
        }
        let (ny, nx) = (ny as usize, nx as usize);
        if !board.in_bounds(ny, nx) {
            return Err(SolveError::InvalidMove("move leaves the board".into()));
        }

        let target = board.cell(ny, nx);
        if target & WALL != 0 {
            return Err(SolveError::InvalidMove("move into a wall".into()));
        }

        if target & BOX != 0 {
            let by = ny as isize + DELTA_Y[dir];
            let bx = nx as isize + DELTA_X[dir];
            if by < 0 || bx < 0 {
                return Err(SolveError::InvalidMove("push leaves the board".into()));
            }
            let (by, bx) = (by as usize, bx as usize);
            if !board.in_bounds(by, bx) {
                return Err(SolveError::InvalidMove("push leaves the board".into()));
            }
            let beyond = board.cell(by, bx);
            if beyond & OCCUPIED != 0 {
                return Err(SolveError::InvalidMove("push into an obstacle".into()));
            }
            board.and_not(ny, nx, BOX);
            board.or(by, bx, BOX);
            stats.pushes += 1;
        }

        board.and_not(py, px, SOKOBAN);
        board.or(ny, nx, SOKOBAN);
        py = ny;
        px = nx;
        stats.moves += 1;
    }

    let mut boxes = 0;
    let mut on_targets = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX != 0 {
                boxes += 1;
                if board.cell(y, x) & TARGET != 0 {
                    on_targets += 1;
                }
            }
        }
    }
    stats.boxes_total = boxes;
    stats.boxes_on_targets = on_targets;
    stats.solved = boxes > 0 && on_targets == boxes;
    Ok(stats)
}

pub fn validate(initial: &Board, lurd: &str) -> Result<LurdStats> {
    replay(initial, lurd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    #[test]
    fn replays_a_single_push() {
        let board = parse_levels("#######\n# .$@ #\n#######\n")
            .remove(0)
            .board
            .unwrap();
        let stats = replay(&board, "L").unwrap();
        assert_eq!(stats.moves, 1);
        assert_eq!(stats.pushes, 1);
        assert!(stats.solved);
    }

    #[test]
    fn rejects_walking_into_walls() {
        let board = parse_levels("#####\n#@$.#\n#####\n")
            .remove(0)
            .board
            .unwrap();
        assert!(replay(&board, "U").is_err());
    }

    #[test]
    fn rejects_push_into_wall() {
        let board = parse_levels("#####\n#@$##\n#####\n")
            .remove(0)
            .board
            .unwrap();
        assert!(replay(&board, "R").is_err());
    }

    #[test]
    fn ignores_whitespace() {
        let board = parse_levels("#######\n# .$@ #\n#######\n")
            .remove(0)
            .board
            .unwrap();
        let stats = replay(&board, " \nL\n").unwrap();
        assert!(stats.solved);
    }
}
