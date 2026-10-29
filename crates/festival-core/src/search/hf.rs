//! HF / REV / BICON / MAX_DIST scoring (`hf_search.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::direction::deadlock_in_direction;
use crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls;
use crate::deadlock::state::DeadlockState;
use crate::error::Result;
use crate::moves::Move;
use crate::planning::imagine;
use crate::search::helper::Helper;
use crate::search::score::{ScoreElement, get_scored_distance, is_better_score};
use crate::solver::LevelContext;

/// Port of `can_be_accessed_from_some_direction`.
fn can_be_accessed_from_some_direction(board: &Board, y: usize, x: usize) -> bool {
    for dir in 0..4 {
        if board.at_dir(y, x, dir) == WALL {
            continue;
        }
        let y2 = y as isize + DELTA_Y[dir] * 2;
        let x2 = x as isize + DELTA_X[dir] * 2;
        if board.get(y2, x2) & OCCUPIED != 0 {
            continue;
        }
        return true;
    }
    false
}

/// Port of `hf_hotspots`.
pub fn hf_hotspots(helper: &Helper, board: &Board, pull_mode: bool) -> i32 {
    let mut sum = 0;

    if pull_mode {
        for y in 0..board.height() {
            for x in 0..board.width() {
                if board.cell(y, x) & BOX != 0 && !can_be_accessed_from_some_direction(board, y, x)
                {
                    sum += 1;
                }
            }
        }
        return sum;
    }

    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX != 0 && helper.imagined_hf_board.cell(y, x) & BOX == 0 {
                for dir in 0..4 {
                    if board.at_dir(y, x, dir) & OCCUPIED != 0 {
                        sum += 1;
                        break;
                    }
                }
            }

            if helper.imagined_hf_board.cell(y, x) & BOX != 0
                && board.cell(y, x) & BOX == 0
                && !can_be_accessed_from_some_direction(board, y, x)
            {
                sum += 1;
            }
        }
    }

    sum
}

/// Port of `is_better_hf_score`.
pub fn is_better_hf_score(
    new_score: &ScoreElement,
    old_score: &ScoreElement,
    pull_mode: bool,
) -> bool {
    if !pull_mode {
        if new_score.dist_to_imagined < INF as i32 && old_score.dist_to_imagined >= INF as i32 {
            return true;
        }
        if new_score.dist_to_imagined >= INF as i32 && old_score.dist_to_imagined < INF as i32 {
            return false;
        }

        if new_score.connectivity < old_score.connectivity {
            return true;
        }
        if new_score.connectivity > old_score.connectivity {
            return false;
        }
        if new_score.imagine > old_score.imagine {
            return true;
        }
        if new_score.imagine < old_score.imagine {
            return false;
        }
        if new_score.hotspots < old_score.hotspots {
            return true;
        }
        if new_score.hotspots > old_score.hotspots {
            return false;
        }
        if new_score.dist_to_imagined < old_score.dist_to_imagined {
            return true;
        }
        if new_score.dist_to_imagined > old_score.dist_to_imagined {
            return false;
        }
        return false;
    }

    if new_score.connectivity < old_score.connectivity {
        return true;
    }
    if new_score.connectivity > old_score.connectivity {
        return false;
    }
    if new_score.rooms_score < old_score.rooms_score {
        return true;
    }
    if new_score.rooms_score > old_score.rooms_score {
        return false;
    }
    if new_score.connectivity == 1 && old_score.connectivity == 1 {
        if new_score.hotspots < old_score.hotspots {
            return true;
        }
        if new_score.hotspots > old_score.hotspots {
            return false;
        }
        if new_score.biconnect < old_score.biconnect {
            return true;
        }
        if new_score.biconnect > old_score.biconnect {
            return false;
        }
    }
    if new_score.dist_to_targets > old_score.dist_to_targets {
        return true;
    }
    if new_score.dist_to_targets < old_score.dist_to_targets {
        return false;
    }
    false
}

/// Port of `is_better_rev_score`.
pub fn is_better_rev_score(
    new_score: &ScoreElement,
    old_score: &ScoreElement,
    pull_mode: bool,
) -> bool {
    if pull_mode {
        if new_score.dist_to_imagined < INF as i32 && old_score.dist_to_imagined >= INF as i32 {
            return true;
        }
        if new_score.dist_to_imagined >= INF as i32 && old_score.dist_to_imagined < INF as i32 {
            return false;
        }
        if new_score.connectivity < old_score.connectivity {
            return true;
        }
        if new_score.connectivity > old_score.connectivity {
            return false;
        }
        if new_score.connectivity > 1 {
            if new_score.dist_to_targets > old_score.dist_to_targets {
                return true;
            }
            if new_score.dist_to_targets < old_score.dist_to_targets {
                return false;
            }
        }
        if new_score.boxes_on_targets > old_score.boxes_on_targets {
            return true;
        }
        if new_score.boxes_on_targets < old_score.boxes_on_targets {
            return false;
        }
        if new_score.dist_to_imagined < old_score.dist_to_imagined {
            return true;
        }
        if new_score.dist_to_imagined > old_score.dist_to_imagined {
            return false;
        }
        return false;
    }

    if new_score.connectivity < old_score.connectivity {
        return true;
    }
    if new_score.connectivity > old_score.connectivity {
        return false;
    }
    if new_score.rooms_score < old_score.rooms_score {
        return true;
    }
    if new_score.rooms_score > old_score.rooms_score {
        return false;
    }
    if new_score.hotspots < old_score.hotspots {
        return true;
    }
    if new_score.hotspots > old_score.hotspots {
        return false;
    }
    if new_score.dist_to_targets > old_score.dist_to_targets {
        return true;
    }
    if new_score.dist_to_targets < old_score.dist_to_targets {
        return false;
    }
    false
}

/// Port of `set_rev_advisor`; returns the `pack` advisor index.
#[allow(clippy::too_many_arguments)]
pub fn set_rev_advisor(
    ctx: &LevelContext,
    board: &Board,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    pull_mode: bool,
    search_mode: SearchMode,
    already_expanded: &[bool],
) -> i32 {
    let mut score = *base_score;
    let mut pack = -1i32;

    for (i, mv) in moves.iter().enumerate() {
        if !pull_mode {
            let (y, x) = ctx.level.coord(mv.to as usize);
            if ctx.level.target_holes.cell(y, x) != 0 {
                return i as i32;
            }
        }

        if already_expanded.get(i).copied().unwrap_or(false) {
            continue;
        }

        if is_better_score(&scores[i], &score, pull_mode, search_mode) {
            score = scores[i];
            pack = i as i32;
        }
    }

    let _ = board;
    pack
}

/// Port of `setup_rev_board`.
pub fn setup_rev_board(
    ctx: &LevelContext,
    state: &DeadlockState,
    board: &Board,
    helper: &mut Helper,
) -> Result<()> {
    helper.imagined_hf_board = *board;

    let w = turn_fixed_boxes_into_walls(
        &ctx.level,
        &ctx.distances,
        &helper.imagined_hf_board,
        &state.envelopes,
    );

    helper.wallified.zero();
    for y in 0..w.height() {
        for x in 0..w.width() {
            if w.cell(y, x) == WALL && ctx.level.is_inner(y, x) {
                helper.wallified.set(y, x, 1);
            }
        }
    }
    Ok(())
}

/// Port of `set_rev_distance`; updates `dist_to_imagined`.
pub fn set_rev_distance(
    ctx: &LevelContext,
    helper: &Helper,
    board: &Board,
    base_score: &mut ScoreElement,
    moves: &[Move],
    scores: &mut [ScoreElement],
) {
    crate::analysis::match_distance::compute_rev_distance(
        &ctx.level,
        board,
        &helper.imagined_hf_board,
        ctx.distances.from_to_slice(),
        base_score,
        moves,
        scores,
    );
}

fn can_be_pushed_to_direction(
    ctx: &LevelContext,
    board: &Board,
    y: usize,
    x: usize,
    dir: usize,
) -> bool {
    if board.at_dir(y, x, dir) & OCCUPIED != 0 {
        return false;
    }
    if board.at_dir(y, x, (dir + 2) & 3) & OCCUPIED != 0 {
        return false;
    }
    if deadlock_in_direction(&ctx.level, &ctx.distances, board, y, x, dir) {
        return false;
    }
    true
}

/// Port of `get_rev_hotspots`.
fn get_rev_hotspots(ctx: &LevelContext, board: &Board) -> Board {
    let mut hs = Board::new(board.width(), board.height()).expect("same dims");

    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            if board.cell(y, x) & TARGET != 0 {
                continue;
            }
            let mut pushable = false;
            for dir in 0..4 {
                if can_be_pushed_to_direction(ctx, board, y, x, dir) {
                    pushable = true;
                    break;
                }
            }
            if !pushable {
                hs.set(y, x, 1);
            }
        }
    }
    hs
}

fn rev_hotspots(ctx: &LevelContext, board: &Board) -> i32 {
    let hs = get_rev_hotspots(ctx, board);
    hs.cells()
        .iter()
        .take(MAX_INNER)
        .filter(|value| **value != 0)
        .count() as i32
}

/// Port of `rev_hotspots_distance`.
fn rev_hotspots_distance(ctx: &LevelContext, helper: &Helper, board: &Board) -> i32 {
    let mut hs = get_rev_hotspots(ctx, board);

    if hs.cells().iter().take(MAX_INNER).all(|value| *value == 0) {
        return get_scored_distance(helper, board);
    }

    let mut b2 = ctx.level.initial;
    b2.clear_boxes_inplace();
    let mut dist = [INF; MAX_INNER];
    crate::bfs::expand_group(&ctx.level, &b2, &mut hs, &mut dist);

    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX != 0 {
                sum += dist[Board::flat_index(y, x)] as i32;
            }
        }
    }
    sum
}

/// Port of `set_score_in_rev_mode`.
pub fn set_score_in_rev_mode(
    ctx: &LevelContext,
    helper: &Helper,
    board: &Board,
    score: &mut ScoreElement,
    pull_mode: bool,
) {
    if !pull_mode {
        score.rooms_score = ctx.rooms.score(&ctx.level, board) as i16;
        score.hotspots = rev_hotspots(ctx, board) as i16;
        score.dist_to_targets = rev_hotspots_distance(ctx, helper, board);
        return;
    }

    score.boxes_on_targets = imagine::compare_to_imagined_hf(board, helper) as i16;
    score.dist_to_targets = get_scored_distance(helper, board);
}

/// Port of `compare_to_imagined_hf`.
pub fn compare_to_imagined_hf(board: &Board, helper: &Helper) -> i32 {
    imagine::compare_to_imagined_hf(board, helper)
}

/// Port of `fill_hf_scores`.
pub fn fill_hf_scores(
    ctx: &LevelContext,
    helper: &Helper,
    board: &Board,
    score: &mut ScoreElement,
    pull_mode: bool,
    search_mode: SearchMode,
) {
    score.boxes_on_targets = 0;
    score.hotspots = hf_hotspots(helper, board, pull_mode) as i16;

    if !pull_mode {
        score.imagine = compare_to_imagined_hf(board, helper) as i16;
        return;
    }

    if matches!(
        search_mode,
        SearchMode::BiconSearch | SearchMode::MaxDistSearch
    ) {
        score.biconnect = crate::analysis::biconnect::biconnect_score(&ctx.level, board) as i16;
    }

    score.rooms_score = ctx.rooms.score(&ctx.level, board) as i16;
    score.dist_to_targets = get_scored_distance(helper, board);
}
