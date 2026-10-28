// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Box move generation and application (`moves.cpp`).

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::corral::detect_corral;
use crate::deadlock::corral_search::check_for_deadlock_zone_moves_at;
use crate::deadlock::direction::deadlock_in_direction;
use crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls;
use crate::deadlock::k_dist::check_for_k_dist_moves;
use crate::deadlock::oracle::check_deadlock_in_moves;
use crate::deadlock::rooms::reduce_moves_in_rooms_deadlock_search;
use crate::deadlock::state::DeadlockState;
use crate::deadlock::wobblers::reduce_moves_in_wobblers_search;
use crate::deadlock::xy::reduce_moves_in_xy_search;
use crate::deadlock::zone::update_deadlock_zone;
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

fn detect_kill_moves(
    ctx: &LevelContext,
    deadlock: &DeadlockState,
    board_in: &Board,
    board: &Board,
    moves: &mut Vec<Move>,
    search_mode: SearchMode,
    helper: &crate::search::helper::Helper,
) -> usize {
    let mut moves_num = moves.len();
    let mut slice = std::mem::take(moves);

    if search_mode == SearchMode::DeadlockSearch {
        moves_num = check_for_deadlock_zone_moves_at(ctx, board_in, &mut slice, moves_num);
    }
    if search_mode == SearchMode::KDistSearch {
        moves_num = check_for_k_dist_moves(ctx, board, &mut slice, moves_num, helper.k_dist_value);
    }
    if search_mode == SearchMode::WobblersSearch {
        moves_num = reduce_moves_in_wobblers_search(ctx, &mut slice, moves_num);
    }
    if search_mode == SearchMode::RoomsDeadlockSearch {
        moves_num = reduce_moves_in_rooms_deadlock_search(
            ctx,
            deadlock,
            &mut slice,
            moves_num,
            helper.tested_deadlock_room,
        );
    }
    if search_mode == SearchMode::XySearch {
        moves_num = reduce_moves_in_xy_search(ctx, board, &mut slice, moves_num);
    }

    slice.truncate(moves_num);
    *moves = slice;
    moves_num
}

/// Port of `find_possible_moves`.
pub fn find_possible_moves(
    ctx: &LevelContext,
    board_in: &Board,
    pull_mode: bool,
    search_mode: SearchMode,
    helper: &mut crate::search::helper::Helper,
    deadlock: &mut DeadlockState,
) -> Result<(Vec<Move>, bool)> {
    let level = &ctx.level;

    let mut b = *board_in;
    b.clear_deadlock_zone_inplace();
    let board = if pull_mode {
        b
    } else {
        turn_fixed_boxes_into_walls(level, &ctx.distances, &b, &deadlock.envelopes)
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
    } else if search_mode == SearchMode::BaseSearch {
        possible = crate::planning::heuristics::compute_possible_from_bases(ctx, &board)?;
    }

    let mut use_corral = !matches!(
        search_mode,
        SearchMode::DeadlockSearch
            | SearchMode::RoomsDeadlockSearch
            | SearchMode::MiniSearch
            | SearchMode::XySearch
            | SearchMode::KDistSearch
    );
    let mut effective_mode = search_mode;
    if search_mode == SearchMode::IgnoreCorral {
        use_corral = false;
        effective_mode = SearchMode::Normal;
    }
    if helper.perimeter_found || helper.enable_inefficient_moves {
        use_corral = false;
    }

    let mut has_corral = false;
    let mut corral_options = *board_in;
    if use_corral && !pull_mode {
        if let Some(options) = detect_corral(level, &ctx.distances, &board) {
            has_corral = true;
            corral_options = options;
        }
    }

    let mut scratch = std::mem::take(&mut helper.move_scratch);
    if scratch.len() < MAX_MOVES {
        scratch.resize(MAX_MOVES, Move::default());
    }
    let mut graph = helper
        .graph_scratch
        .take()
        .unwrap_or_else(|| Graph::new(level));
    let mut moves: Vec<Move> = Vec::new();

    let result = (|| -> Result<(Vec<Move>, bool)> {
        for index in 0..level.index_num() {
            let (y, x) = level.coord(index);

            if has_corral && corral_options.cell(y, x) & BOX == 0 {
                continue;
            }
            if search_mode == SearchMode::RevSearch
                && pull_mode
                && !helper.perimeter_found
                && helper.wallified.cell(y, x) != 0
            {
                continue;
            }
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
            if matches!(
                effective_mode,
                SearchMode::Normal | SearchMode::WobblersSearch
            ) {
                graph.find_reversible_moves(level, &board, index, pull_mode);
            }

            let mut count = graph.get_box_moves(level, index, &mut scratch);
            for mv in scratch.iter_mut().take(count) {
                mv.pull = pull_mode;
            }

            let valid = check_deadlock_in_moves(
                ctx,
                deadlock,
                &board,
                &scratch[..count],
                pull_mode,
                effective_mode,
                helper,
            )?;
            let mut kept = 0usize;
            for i in 0..count {
                if valid[i] {
                    scratch[kept] = scratch[i];
                    kept += 1;
                }
            }
            count = kept;

            let mut box_moves: Vec<Move> = scratch[..count].to_vec();
            count = detect_kill_moves(
                ctx,
                deadlock,
                board_in,
                &board,
                &mut box_moves,
                effective_mode,
                helper,
            );

            if count == 1 && box_moves[0].kill {
                moves.clear();
                moves.push(box_moves[0]);
                break;
            }

            moves.extend_from_slice(&box_moves[..count]);

            if moves.len() > MAX_MOVES - MAX_INNER {
                return Err(SolveError::TooManyMoves);
            }
        }

        if search_mode == SearchMode::BaseSearch {
            moves =
                crate::planning::heuristics::mark_sink_squares_moves(ctx, deadlock, &board, moves)?;
        }

        Ok((moves, has_corral))
    })();
    helper.move_scratch = scratch;
    helper.graph_scratch = Some(graph);
    result
}

/// Port of `apply_move`.
pub fn apply_move(board: &mut Board, mv: &Move, search_mode: SearchMode, level: &Level) {
    debug_assert_ne!(search_mode as i32, -1);

    if mv.kill {
        let (y, x) = level.coord(mv.from as usize);
        board.and_not(y, x, BOX);
        board.expand_sokoban_cloud();
        if search_mode == SearchMode::DeadlockSearch {
            update_deadlock_zone(level, board, mv.pull);
        }
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

    if search_mode == SearchMode::DeadlockSearch {
        update_deadlock_zone(level, board, mv.pull);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::solver::SearchClock;
    use std::time::Duration;

    fn context(text: &str) -> (LevelContext, DeadlockState) {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
        let ctx = LevelContext::analyse(level, &clock).unwrap();
        let state = DeadlockState::prepare(&ctx).unwrap();
        (ctx, state)
    }

    #[test]
    fn generates_macro_pushes() {
        let (ctx, mut deadlock) = context("#######\n#@ $ .#\n#######\n");
        let mut helper = crate::search::helper::Helper::default();
        let (moves, _has_corral) = find_possible_moves(
            &ctx,
            &ctx.level.board,
            false,
            SearchMode::Normal,
            &mut helper,
            &mut deadlock,
        )
        .unwrap();
        // The one-step push to (1,4) is pruned as an inefficient position, so
        // only the macro push onto the target remains.
        assert_eq!(moves.len(), 1);
        assert!(
            moves
                .iter()
                .any(|mv| mv.to == ctx.level.index_at(1, 5).unwrap() as u16)
        );
    }

    #[test]
    fn apply_move_pushes_the_box() {
        let (ctx, mut deadlock) = context("#######\n#@ $ .#\n#######\n");
        let mut helper = crate::search::helper::Helper::default();
        let (moves, _) = find_possible_moves(
            &ctx,
            &ctx.level.board,
            false,
            SearchMode::Normal,
            &mut helper,
            &mut deadlock,
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
