// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Box move generation and application (`moves.cpp`).

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::direction::deadlock_in_direction;
use crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls;
use crate::deadlock::oracle::DeadlockOracle;
use crate::error::{Result, SolveError};
use crate::graph::Graph;
use crate::level::Level;
use crate::solver::LevelContext;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveAttr {
    pub advisor: bool,
    /// Perimeter can make this negative, hence the wider type.
    pub weight: i16,
    pub reversible: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Move {
    pub from: u16,
    pub to: u16,
    pub sokoban_position: u8,
    pub kill: bool,
    pub base: bool,
    pub pull: bool,
    pub attr: MoveAttr,
}

pub fn attr_to_weight(attr: &MoveAttr) -> i16 {
    if attr.advisor { 0 } else { 1 }
}

pub fn box_can_be_moved(
    level: &Level,
    distances: &crate::distance::Distances,
    board: &Board,
    index: usize,
    pull_mode: bool,
) -> bool {
    let (y, x) = level.coord(index);

    if !pull_mode {
        for dir in 0..4 {
            if board.at_dir(y, x, dir) & OCCUPIED != 0 {
                continue;
            }
            let sok_y = y as isize - DELTA_Y[dir];
            let sok_x = x as isize - DELTA_X[dir];
            if board.get(sok_y, sok_x) & SOKOBAN == 0 {
                continue;
            }
            if deadlock_in_direction(level, distances, board, y, x, dir) {
                continue;
            }
            return true;
        }
        return false;
    }

    for dir in 0..4 {
        if board.at_dir(y, x, dir) & SOKOBAN == 0 {
            continue;
        }
        let next_y = y as isize + DELTA_Y[dir] * 2;
        let next_x = x as isize + DELTA_X[dir] * 2;
        if board.get(next_y, next_x) & OCCUPIED != 0 {
            continue;
        }
        return true;
    }
    false
}

/// Port of `find_possible_moves`. `has_corral` is always false in Phase 2
/// because corral detection belongs to the deadlock subsystem.
pub fn find_possible_moves(
    ctx: &LevelContext,
    board_in: &Board,
    pull_mode: bool,
    search_mode: SearchMode,
    _helper: &mut crate::search::helper::Helper,
) -> Result<(Vec<Move>, bool)> {
    let level = &ctx.level;

    let mut cleaned = *board_in;
    cleaned.clear_deadlock_zone_inplace();
    let board = if pull_mode {
        cleaned
    } else {
        turn_fixed_boxes_into_walls(level, &ctx.distances, &cleaned, &ctx.envelopes)
    };

    let mut possible = Board::new(board.width(), board.height())?;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if level.is_inner(y, x) {
                possible.set(y, x, 1);
            }
        }
    }
    if !pull_mode {
        possible = bfs::get_srcs_of_all_targets(level, &board);
    }

    let mut graph = Graph::new(level);
    let mut scratch = vec![Move::default(); MAX_MOVES];
    let mut moves: Vec<Move> = Vec::new();

    for index in 0..level.index_num() {
        let (y, x) = level.coord(index);
        if board.cell(y, x) & BOX == 0 {
            continue;
        }
        if !box_can_be_moved(level, &ctx.distances, &board, index, pull_mode) {
            continue;
        }

        graph.set_current_impossible(
            level,
            &board,
            y,
            x,
            pull_mode,
            &possible,
            ctx.distances.impossible_slice(),
        );
        graph.mark_box_moves_in_graph(level, &board, index, pull_mode);
        if matches!(search_mode, SearchMode::Normal | SearchMode::WobblersSearch) {
            graph.find_reversible_moves(level, &board, index, pull_mode);
        }

        let mut count = graph.get_box_moves(level, index, &mut scratch);
        for mv in scratch.iter_mut().take(count) {
            mv.pull = pull_mode;
        }

        let valid = DeadlockOracle::new(level).check_deadlock_in_moves(
            &ctx.distances,
            &board,
            &scratch[..count],
            pull_mode,
        );
        let mut kept = 0usize;
        for i in 0..count {
            if valid[i] {
                scratch[kept] = scratch[i];
                kept += 1;
            }
        }
        count = kept;

        if count == 1 && scratch[0].kill {
            return Ok((vec![scratch[0]], false));
        }

        moves.extend_from_slice(&scratch[..count]);

        if moves.len() > MAX_MOVES - MAX_INNER {
            return Err(SolveError::TooManyMoves);
        }
    }

    Ok((moves, false))
}

/// Port of `apply_move`. Search-mode specific zone bookkeeping is deferred to
/// the phases that introduce those modes.
pub fn apply_move(board: &mut Board, mv: &Move, search_mode: SearchMode, level: &Level) {
    debug_assert_ne!(search_mode as i32, -1);

    if mv.kill {
        let (y, x) = level.coord(mv.from as usize);
        board.and_not(y, x, BOX);
        board.expand_sokoban_cloud();
        return;
    }

    board.clear_sokoban_inplace();

    let (from_y, from_x) = level.coord(mv.from as usize);
    if board.cell(from_y, from_x) & BOX == 0 {
        debug_assert!(false, "apply_move: missing box");
        return;
    }
    board.and_not(from_y, from_x, BOX);

    let (to_y, to_x) = level.coord(mv.to as usize);
    if board.cell(to_y, to_x) & OCCUPIED != 0 {
        debug_assert!(false, "apply_move: occupied destination");
        return;
    }
    board.or(to_y, to_x, BOX);

    if mv.base {
        board.and_not(to_y, to_x, BOX);
        if board.cell(to_y, to_x) & BASE == 0 {
            debug_assert!(false, "apply_move: missing base");
            return;
        }
        board.and_not(to_y, to_x, BASE);
    }

    let dir = mv.sokoban_position as usize;
    let sok_y = to_y as isize + DELTA_Y[dir];
    let sok_x = to_x as isize + DELTA_X[dir];
    if sok_y >= 0 && sok_x >= 0 {
        board.or(sok_y as usize, sok_x as usize, SOKOBAN);
    }

    board.expand_sokoban_cloud();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::solver::SearchClock;
    use std::time::Duration;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    #[test]
    fn generates_macro_pushes() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let mut helper = crate::search::helper::Helper::default();
        let (moves, has_corral) = find_possible_moves(
            &ctx,
            &ctx.level.board,
            false,
            SearchMode::Normal,
            &mut helper,
        )
        .unwrap();
        assert!(!has_corral);
        // Two pushes reachable in this corridor: one box move to (1,4), and
        // the macro move all the way to (1,5).
        assert_eq!(moves.len(), 2);
        assert!(
            moves
                .iter()
                .any(|mv| mv.to == ctx.level.index_at(1, 5).unwrap() as u16)
        );
    }

    #[test]
    fn apply_move_pushes_the_box() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let mut helper = crate::search::helper::Helper::default();
        let (moves, _) = find_possible_moves(
            &ctx,
            &ctx.level.board,
            false,
            SearchMode::Normal,
            &mut helper,
        )
        .unwrap();
        let mut board = ctx.level.board;
        apply_move(&mut board, &moves[0], SearchMode::Normal, &ctx.level);
        let (to_y, to_x) = ctx.level.coord(moves[0].to as usize);
        assert_eq!(board.cell(to_y, to_x) & BOX, BOX);
        let (from_y, from_x) = ctx.level.coord(moves[0].from as usize);
        assert_eq!(board.cell(from_y, from_x) & BOX, 0);
    }
}
