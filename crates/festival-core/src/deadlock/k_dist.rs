//! k-distance deadlock search (`k_dist_deadlock.cpp`).

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::positions::PositionSet;
use crate::deadlock::state::DeadlockState;
use crate::error::Result;
use crate::moves::{Move, apply_move};
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

const K_DIST_MAX_POSITIONS: usize = 600;
const MAX_K_DIST_HASH_SIZE: usize = 500;

pub struct KDistEntry {
    pub hash: u64,
    pub forced: Board,
    pub adj_4: Board,
    pub adj_9: Board,
}

pub struct KDistHash {
    pub entries: Vec<KDistEntry>,
}

impl KDistHash {
    pub fn new(width: usize, height: usize) -> Self {
        let _ = (width, height);
        Self {
            entries: Vec::with_capacity(MAX_K_DIST_HASH_SIZE),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn find(&self, hash: u64) -> Option<usize> {
        self.entries.iter().position(|entry| entry.hash == hash)
    }
}

fn put_sokoban_in_zone(ctx: &LevelContext, board: &mut Board, zones: &[u32; MAX_INNER], z: u32) {
    board.clear_sokoban_inplace();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if zones[Board::flat_index(y, x)] == z && board.cell(y, x) & OCCUPIED == 0 {
                board.or(y, x, SOKOBAN);
            }
        }
    }
    board.expand_sokoban_cloud();
    let _ = ctx;
}

pub fn sokoban_touches_a_filled_hole(ctx: &LevelContext, board: &Board) -> bool {
    let Some((sok_y, sok_x)) = board.sokoban_position() else {
        return false;
    };
    for i in 0..4 {
        let y = sok_y as isize + DELTA_Y[i];
        let x = sok_x as isize + DELTA_X[i];
        if board.get(y, x) == WALL && ctx.level.initial.get(y, x) != WALL {
            return true;
        }
    }
    false
}

fn count_adjacent_boxes(board: &Board) -> (Board, Board) {
    let mut adj_4 = Board::new(board.width(), board.height()).expect("same dims");
    let mut adj_9 = Board::new(board.width(), board.height()).expect("same dims");

    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            adj_4.set(y, x, adj_4.cell(y, x) + 1);
            adj_9.set(y, x, adj_9.cell(y, x) + 1);
            for k in 0..8 {
                let ny = y as isize + DELTA_Y_8[k];
                let nx = x as isize + DELTA_X_8[k];
                if ny < 0 || nx < 0 {
                    continue;
                }
                let (ny, nx) = (ny as usize, nx as usize);
                if !board.in_bounds(ny, nx) {
                    continue;
                }
                adj_9.set(ny, nx, adj_9.cell(ny, nx) + 1);
                if k & 1 == 0 {
                    adj_4.set(ny, nx, adj_4.cell(ny, nx) + 1);
                }
            }
        }
    }
    (adj_4, adj_9)
}

fn is_better_k_dist_score(new_score: &ScoreElement, old_score: &ScoreElement) -> bool {
    if new_score.boxes_in_level < old_score.boxes_in_level {
        return true;
    }
    if new_score.boxes_in_level > old_score.boxes_in_level {
        return false;
    }
    if new_score.connectivity < old_score.connectivity {
        return true;
    }
    if new_score.connectivity > old_score.connectivity {
        return false;
    }
    false
}

fn get_k_dist_pos_and_son(positions: &PositionSet) -> Option<(usize, usize)> {
    let mut best = None;
    let mut best_score = ScoreElement::default();
    for i in 0..positions.len() {
        let p = &positions.positions[i];
        if p.position_deadlocked {
            continue;
        }
        for j in 0..p.moves.len() {
            if p.move_deadlocked[j] || p.place_in_positions[j].is_some() {
                continue;
            }
            if best.is_none() || is_better_k_dist_score(&p.scores[j], &best_score) {
                best = Some((i, j));
                best_score = p.scores[j];
            }
        }
    }
    best
}

pub fn check_for_k_dist_moves(
    ctx: &LevelContext,
    board: &Board,
    moves: &mut [Move],
    moves_num: usize,
    k_dist_value: i32,
) -> usize {
    for i in 0..moves_num {
        let from = moves[i].from as usize;
        let to = moves[i].to as usize;
        if (ctx.distances.bfs_from_to(to, from) as i32) < k_dist_value {
            continue;
        }

        let mut c = *board;
        apply_move(&mut c, &moves[i], SearchMode::Normal, &ctx.level);
        let (y, x) = ctx.level.coord(from);
        if c.cell(y, x) & SOKOBAN != 0 {
            moves[0] = moves[i];
            moves[0].kill = true;
            return 1;
        }
    }
    moves_num
}

struct KDistForced {
    terminated: bool,
    forced: Board,
    adj_4: Board,
    adj_9: Board,
}

fn get_k_dist_forced_boxes(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board_in: &Board,
    k_dist_val: i32,
) -> Result<KDistForced> {
    let mut helper = Helper {
        k_dist_value: k_dist_val,
        ..Helper::default()
    };

    let mut b = *board_in;
    b.go_to_final_position_with_given_walls();

    let mut zones = [INF; MAX_INNER];
    let start_zones_num = bfs::mark_connectivities(&ctx.level, &b, &mut zones);

    let mut positions = PositionSet::new(K_DIST_MAX_POSITIONS);
    for i in 0..start_zones_num {
        let mut c = b;
        put_sokoban_in_zone(ctx, &mut c, &zones, i as u32);
        positions.set_first_position(ctx, &c, true, SearchMode::KDistSearch, &mut helper, state)?;
    }
    for i in 0..positions.len() {
        positions.direct_other_sons_to_position(i);
    }

    let mut removed_all = false;
    while positions.len() < K_DIST_MAX_POSITIONS {
        let Some((pos, son)) = get_k_dist_pos_and_son(&positions) else {
            break;
        };
        positions.expand_position(ctx, pos, son, true, &mut helper, state)?;
        let last = positions.len() - 1;
        if positions.positions[last].position_score.boxes_in_level == 0 {
            removed_all = true;
            break;
        }
    }

    let mut forced = Board::new(board_in.width(), board_in.height()).expect("same dims");
    let mut adj_4 = forced;
    let mut adj_9 = forced;
    let terminated = positions.len() >= K_DIST_MAX_POSITIONS;

    if terminated || removed_all {
        return Ok(KDistForced {
            terminated,
            forced,
            adj_4,
            adj_9,
        });
    }

    for y in 0..forced.height() {
        for x in 0..forced.width() {
            forced.set(y, x, 1);
            adj_4.set(y, x, 10);
            adj_9.set(y, x, 10);
        }
    }

    for pos in 0..positions.len() {
        let board = positions.positions[pos].board;
        let (local_4, local_9) = count_adjacent_boxes(&board);
        for y in 0..board.height() {
            for x in 0..board.width() {
                if board.cell(y, x) & BOX == 0 {
                    forced.set(y, x, 0);
                }
                if local_4.cell(y, x) < adj_4.cell(y, x) {
                    adj_4.set(y, x, local_4.cell(y, x));
                }
                if local_9.cell(y, x) < adj_9.cell(y, x) {
                    adj_9.set(y, x, local_9.cell(y, x));
                }
            }
        }
    }

    Ok(KDistForced {
        terminated,
        forced,
        adj_4,
        adj_9,
    })
}

fn check_deadlock_using_k_dist_cache(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
) -> Result<bool> {
    let mut wallified = crate::deadlock::fixed_boxes::turn_fixed_boxes_into_walls(
        &ctx.level,
        &ctx.distances,
        board,
        &state.envelopes,
    );
    crate::analysis::holes::turn_target_holes_into_walls(&mut wallified, &ctx.level.target_holes);

    let mut w = wallified;
    w.clear_boxes_inplace();
    w.clear_sokoban_inplace();
    let hash = w.hash();

    if state.k_dist.find(hash).is_none() {
        let first = get_k_dist_forced_boxes(ctx, state, &w, 5)?;
        let mut result = first;
        if result.terminated {
            result = get_k_dist_forced_boxes(ctx, state, &w, 3)?;
        }
        if state.k_dist.entries.len() < MAX_K_DIST_HASH_SIZE {
            state.k_dist.entries.push(KDistEntry {
                hash,
                forced: result.forced,
                adj_4: result.adj_4,
                adj_9: result.adj_9,
            });
        }
    }

    let Some(n) = state.k_dist.find(hash) else {
        return Ok(false);
    };

    let (adj_4, adj_9) = count_adjacent_boxes(&wallified);
    let entry = &state.k_dist.entries[n];
    for y in 0..board.height() {
        for x in 0..board.width() {
            if entry.forced.cell(y, x) != 0 && board.cell(y, x) & BOX == 0 {
                return Ok(true);
            }
            if adj_4.cell(y, x) < entry.adj_4.cell(y, x) {
                return Ok(true);
            }
            if adj_9.cell(y, x) < entry.adj_9.cell(y, x) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn is_k_dist_deadlock(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    board: &Board,
) -> Result<bool> {
    if board.all_boxes_in_place(false, &ctx.level.initial) {
        return Ok(false);
    }
    check_deadlock_using_k_dist_cache(ctx, state, board)
}
