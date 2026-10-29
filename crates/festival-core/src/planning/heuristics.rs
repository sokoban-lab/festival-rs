//! Base matching, sink squares and packing advisors (`heuristics.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::oracle::check_matching_deadlock;
use crate::deadlock::state::DeadlockState;
use crate::error::{Result, SolveError};
use crate::graph::Graph;
use crate::moves::Move;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

const BIG_DISTANCE: u32 = 6;

/// Port of `boxes_in_different_rooms`.
pub fn boxes_in_different_rooms(
    ctx: &LevelContext,
    state: &DeadlockState,
    index1: usize,
    index2: usize,
) -> bool {
    if ctx.distances.bfs_from_to(index1, index2) >= BIG_DISTANCE {
        return true;
    }

    let (box1_y, box1_x) = ctx.level.coord(index1);
    let (box2_y, box2_x) = ctx.level.coord(index2);
    let r1 = ctx.rooms.color(box1_y, box1_x);
    let r2 = ctx.rooms.color(box2_y, box2_x);

    if r1 != INF && r2 != INF && r1 != r2 {
        return true;
    }

    if r1 != r2 {
        if r1 != INF {
            if let Some(room) = state.rooms.room_with_corridors.get(r1 as usize) {
                if room.cell(box2_y, box2_x) != 0 {
                    return false;
                }
            }
        }
        if r2 != INF {
            if let Some(room) = state.rooms.room_with_corridors.get(r2 as usize) {
                if room.cell(box1_y, box1_x) != 0 {
                    return false;
                }
            }
        }
        if box1_y != box2_y && box1_x != box2_x {
            return true;
        }
    }

    false
}

/// Port of `get_kill_zone`: the first square far from every SCC base.
pub fn get_kill_zone(ctx: &LevelContext, bases: &Board, kill_zone: &mut Board) {
    let mut bases_list = Vec::new();
    for y in 0..bases.height() {
        for x in 0..bases.width() {
            if bases.cell(y, x) != 0 {
                if let Some(index) = ctx.level.index_at(y, x) {
                    bases_list.push(index);
                }
            }
        }
    }

    kill_zone.zero();
    for i in 0..ctx.level.index_num() {
        let mut all_far = true;
        for &base in &bases_list {
            if ctx.distances.bfs_from_to(i, base) < BIG_DISTANCE {
                all_far = false;
                break;
            }
        }
        if all_far {
            let (y, x) = ctx.level.coord(i);
            kill_zone.set(y, x, 1);
            break;
        }
    }
}

/// Port of `compute_possible_from_bases`.
pub fn compute_possible_from_bases(ctx: &LevelContext, board: &Board) -> Result<Board> {
    let mut without_boxes = ctx.level.initial;
    without_boxes.clear_boxes_inplace();

    let mut bases = Board::new(board.width(), board.height())?;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BASE != 0 {
                bases.set(y, x, 1);
            }
        }
    }

    let mut possible = Board::new(board.width(), board.height())?;
    let mut graph = Graph::new(&ctx.level);
    graph.find_targets_of_a_board_input(&ctx.level, &without_boxes, &bases, &mut possible);
    Ok(possible)
}

/// Port of `test_base_matching`.
fn test_base_matching(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    from: usize,
    to: usize,
) -> Result<bool> {
    let (from_y, from_x) = ctx.level.coord(from);
    let (to_y, to_x) = ctx.level.coord(to);
    debug_assert!(board.cell(from_y, from_x) & BOX != 0);
    debug_assert!(board.cell(to_y, to_x) & BASE != 0);

    let mut c = *board;
    c.and_not(from_y, from_x, BOX);
    c.and_not(to_y, to_x, BASE);

    Ok(!check_matching_deadlock(
        ctx,
        state,
        &c,
        true,
        SearchMode::BaseSearch,
    )?)
}

fn unreachable_area_contains_elim(board: &Board, elim: &Board) -> bool {
    for y in 0..board.height() {
        for x in 0..board.width() {
            if elim.cell(y, x) != 0 && board.cell(y, x) & SOKOBAN == 0 {
                return true;
            }
        }
    }
    false
}

/// Port of `compute_bases_scc`: the largest mutually reachable base group.
pub fn compute_bases_scc(ctx: &LevelContext, board_in: &Board) -> Result<Board> {
    let mut b = *board_in;
    b.turn_boxes_into_walls();

    let mut bases = Vec::new();
    for y in 0..b.height() {
        for x in 0..b.width() {
            if b.cell(y, x) & BASE != 0 {
                if let Some(index) = ctx.level.index_at(y, x) {
                    bases.push(index);
                }
            }
        }
    }
    let n = bases.len();
    if n == 0 {
        return Board::new(b.width(), b.height());
    }

    let mut matrix = vec![false; n * n];
    let mut graph = Graph::new(&ctx.level);
    graph.build(&ctx.level, &b, false);
    let mut targets = Board::new(b.width(), b.height())?;

    for (i, &base) in bases.iter().enumerate() {
        let (y, x) = ctx.level.coord(base);
        graph.find_targets_of_place_without_init_graph(&ctx.level, y, x, &mut targets);
        if targets.cell(y, x) == 0 {
            return Err(SolveError::Internal("base scc: no self".into()));
        }
        for (j, &other) in bases.iter().enumerate() {
            let (oy, ox) = ctx.level.coord(other);
            if targets.cell(oy, ox) != 0 {
                matrix[i * n + j] = true;
            }
        }
    }

    for i in 0..n {
        if !matrix[i * n + i] {
            return Err(SolveError::Internal("base scc: bad diagonal".into()));
        }
    }

    let mut scc = vec![-1i32; n];
    let mut current = 0i32;
    for i in 0..n {
        if scc[i] != -1 {
            continue;
        }
        scc[i] = current;
        for j in i..n {
            if matrix[i * n + j] && matrix[j * n + i] {
                scc[j] = current;
            }
        }
        current += 1;
    }

    let mut scc_size = vec![0usize; n];
    for i in 0..n {
        for j in i..n {
            if scc[j] == scc[i] {
                scc_size[i] += 1;
            }
        }
    }
    let max_size = scc_size.iter().copied().max().unwrap_or(0);

    let mut scc_board = Board::new(b.width(), b.height())?;
    for i in 0..n {
        if scc_size[i] != max_size {
            continue;
        }
        for j in i..n {
            if scc[j] == scc[i] {
                let (y, x) = ctx.level.coord(bases[j]);
                scc_board.set(y, x, 1);
            }
        }
    }
    Ok(scc_board)
}

/// Port of `mark_sink_squares_moves` (BASE_SEARCH).
pub fn mark_sink_squares_moves(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    mut moves: Vec<Move>,
) -> Result<Vec<Move>> {
    let elim = compute_bases_scc(ctx, board)?;
    if unreachable_area_contains_elim(board, &elim) {
        return Ok(moves);
    }

    let mut move_tested = vec![false; moves.len()];
    let mut has_base_move = false;

    for (i, mv) in moves.iter_mut().enumerate() {
        move_tested[i] = true;

        let (to_y, to_x) = ctx.level.coord(mv.to as usize);
        if elim.cell(to_y, to_x) == 0 {
            continue;
        }
        if !boxes_in_different_rooms(ctx, state, mv.from as usize, mv.to as usize) {
            continue;
        }
        if !test_base_matching(ctx, state, board, mv.from as usize, mv.to as usize)? {
            continue;
        }

        mv.base = true;
        has_base_move = true;
        break;
    }

    if !has_base_move {
        for mv in moves.iter_mut() {
            mv.base = false;
        }
        return Ok(moves);
    }

    let mut kept = Vec::with_capacity(moves.len());
    let mut box_was_pulled = vec![false; ctx.level.index_num()];

    for (i, mv) in moves.iter_mut().enumerate() {
        if move_tested[i] && !mv.base {
            continue;
        }

        if mv.base {
            box_was_pulled[mv.from as usize] = true;
            kept.push(*mv);
            continue;
        }

        if box_was_pulled[mv.from as usize] {
            continue;
        }

        let (to_y, to_x) = ctx.level.coord(mv.to as usize);
        if elim.cell(to_y, to_x) == 0 {
            continue;
        }
        if !boxes_in_different_rooms(ctx, state, mv.from as usize, mv.to as usize) {
            continue;
        }
        if !test_base_matching(ctx, state, board, mv.from as usize, mv.to as usize)? {
            continue;
        }

        mv.base = true;
        box_was_pulled[mv.from as usize] = true;
        kept.push(*mv);
    }

    Ok(kept)
}

#[derive(Clone, Copy)]
struct PullCandidate {
    kill_zone: i32,
    connectivity: i32,
    room_connectivity: i32,
    pull_len: i32,
    target_hole: i32,
}

fn is_better_pull_candidate(old: &PullCandidate, new: &PullCandidate) -> bool {
    if new.target_hole > old.target_hole {
        return false;
    }
    if new.target_hole < old.target_hole {
        return true;
    }
    if new.kill_zone > old.kill_zone {
        return true;
    }
    if new.kill_zone < old.kill_zone {
        return false;
    }
    if new.connectivity < old.connectivity {
        return true;
    }
    if new.connectivity > old.connectivity {
        return false;
    }
    if new.room_connectivity < old.room_connectivity {
        return true;
    }
    if new.room_connectivity > old.room_connectivity {
        return false;
    }
    if new.pull_len > old.pull_len {
        return true;
    }
    if new.pull_len < old.pull_len {
        return false;
    }
    false
}

/// Port of `pick_pull_advisor_normal`.
fn pick_pull_advisor_normal(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    search_mode: SearchMode,
) -> Result<i32> {
    let b = *board;
    let mut kill_zone = Board::new(b.width(), b.height())?;

    if search_mode == SearchMode::BaseSearch {
        let scc = compute_bases_scc(ctx, board)?;
        get_kill_zone(ctx, &scc, &mut kill_zone);

        let mut w = b;
        w.turn_boxes_into_walls();
        let mut targets = Board::new(b.width(), b.height())?;
        let mut graph = Graph::new(&ctx.level);
        graph.find_targets_of_a_board_input(&ctx.level, &w, &scc, &mut targets);
        kill_zone.and_board(&targets);
    }

    let mut best_pull = PullCandidate {
        kill_zone: -1,
        target_hole: 2,
        connectivity: 0,
        room_connectivity: 0,
        pull_len: 0,
    };
    let mut best_move = -1i32;

    for (i, mv) in moves.iter().enumerate() {
        if scores[i].connectivity > base_score.connectivity {
            continue;
        }
        if scores[i].rooms_score > base_score.rooms_score {
            continue;
        }
        if scores[i].dist_to_targets <= base_score.dist_to_targets {
            continue;
        }

        let (from_y, from_x) = ctx.level.coord(mv.from as usize);
        let (to_y, to_x) = ctx.level.coord(mv.to as usize);
        if b.cell(from_y, from_x) & TARGET == 0 {
            continue;
        }

        let current = PullCandidate {
            kill_zone: kill_zone.cell(to_y, to_x) as i32,
            connectivity: scores[i].connectivity as i32,
            room_connectivity: scores[i].rooms_score as i32,
            pull_len: ctx.distances.from_to(mv.to as usize, mv.from as usize) as i32,
            target_hole: ctx.level.target_holes.cell(from_y, from_x) as i32,
        };

        if is_better_pull_candidate(&best_pull, &current) {
            best_move = i as i32;
            best_pull = current;
        }
    }

    let _ = state;
    Ok(best_move)
}

struct BasePriority {
    hole: i32,
    connectivity: i32,
    dist: i32,
}

fn is_better_base_priority(new: &BasePriority, old: &BasePriority) -> bool {
    if new.hole < old.hole {
        return true;
    }
    if new.hole > old.hole {
        return false;
    }
    if new.connectivity < old.connectivity {
        return true;
    }
    if new.connectivity > old.connectivity {
        return false;
    }
    if new.dist > old.dist {
        return true;
    }
    if new.dist < old.dist {
        return false;
    }
    false
}

fn connectivity_after_move(ctx: &LevelContext, board: &Board, mv: &Move) -> i32 {
    let mut b = *board;
    let (from_y, from_x) = ctx.level.coord(mv.from as usize);
    let (to_y, to_x) = ctx.level.coord(mv.to as usize);
    b.and_not(from_y, from_x, BOX);
    b.or(to_y, to_x, BOX);
    crate::bfs::get_connectivity(&ctx.level, &b) as i32
}

/// Port of `find_best_base_pull`.
fn find_best_base_pull(ctx: &LevelContext, board: &Board, moves: &[Move]) -> Result<i32> {
    // A box that is not on a target/base is a floater: pulling it is free.
    for (i, mv) in moves.iter().enumerate() {
        if !mv.base {
            continue;
        }
        let (y, x) = ctx.level.coord(mv.from as usize);
        if board.cell(y, x) == BOX {
            return Ok(i as i32);
        }
    }

    let mut best_move = -1i32;
    let mut best_pull = BasePriority {
        hole: 2,
        connectivity: 0,
        dist: 0,
    };

    for (i, mv) in moves.iter().enumerate() {
        if !mv.base {
            continue;
        }
        let (from_y, from_x) = ctx.level.coord(mv.from as usize);
        let current = BasePriority {
            hole: ctx.level.target_holes.cell(from_y, from_x) as i32,
            connectivity: connectivity_after_move(ctx, board, mv),
            dist: ctx.distances.bfs_from_to(mv.to as usize, mv.from as usize) as i32,
        };
        if best_move == -1 || is_better_base_priority(&current, &best_pull) {
            best_move = i as i32;
            best_pull = current;
        }
    }

    if best_move == -1 {
        return Err(SolveError::Internal("base bug".into()));
    }
    Ok(best_move)
}

/// Port of `packing_search_advisor`.
pub fn packing_search_advisor(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    search_mode: SearchMode,
) -> Result<i32> {
    let has_elim = moves.iter().any(|mv| mv.base);
    if !has_elim {
        return pick_pull_advisor_normal(ctx, state, board, base_score, moves, scores, search_mode);
    }
    find_best_base_pull(ctx, board, moves)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;
    use std::time::Duration;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = crate::solver::SearchClock::new(Some(Duration::from_secs(60)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    #[test]
    fn scc_of_a_single_base_is_itself() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let mut b = ctx.level.board;
        b.enter_reverse_mode(&ctx.level.initial, true);
        let scc = compute_bases_scc(&ctx, &b).unwrap();
        let (y, x) = ctx.level.coord(ctx.level.index_at(1, 3).unwrap());
        assert_eq!(scc.cell(y, x), 1);
    }
}
