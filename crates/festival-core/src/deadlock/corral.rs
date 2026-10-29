//! Corral detection (`corral.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::deadlock::direction::deadlock_in_direction;
use crate::distance::Distances;
use crate::level::Level;

const MAX_CORRALS_NUM: usize = 70;

struct CorralData {
    corral: Board,
    corral_d: [u32; MAX_INNER],
    corral_component_used: [bool; MAX_CORRALS_NUM],
    corral_sokoban_comp: u32,
    corral_comps_num: usize,
    corral_candidates: [bool; MAX_CORRALS_NUM],
}

impl CorralData {
    fn new(level: &Level) -> Self {
        Self {
            corral: Board::new(level.initial.width(), level.initial.height()).expect("dims"),
            corral_d: [INF; MAX_INNER],
            corral_component_used: [false; MAX_CORRALS_NUM],
            corral_sokoban_comp: INF,
            corral_comps_num: 0,
            corral_candidates: [false; MAX_CORRALS_NUM],
        }
    }
}

fn get_direction_from_pusher_dest(
    pusher_y: usize,
    pusher_x: usize,
    dest_y: usize,
    dest_x: usize,
) -> Option<(usize, usize, usize)> {
    for dir in 0..4 {
        if pusher_y as isize + DELTA_Y[dir] * 2 == dest_y as isize
            && pusher_x as isize + DELTA_X[dir] * 2 == dest_x as isize
        {
            return Some((
                (pusher_y as isize + DELTA_Y[dir]) as usize,
                (pusher_x as isize + DELTA_X[dir]) as usize,
                dir,
            ));
        }
    }
    None
}

fn get_corral_boxes(orig: &Board, cd: &CorralData) -> Board {
    let mut boxes = *orig;
    boxes.clear_boxes_inplace();
    boxes.clear_sokoban_inplace();

    for y in 0..orig.height() {
        for x in 0..orig.width() {
            if cd.corral.cell(y, x) == 0 {
                continue;
            }
            boxes.or(y, x, SOKOBAN);
            for k in 0..8 {
                let ny = y as isize + DELTA_Y_8[k];
                let nx = x as isize + DELTA_X_8[k];
                if orig.get(ny, nx) & BOX != 0 {
                    boxes.or(ny as usize, nx as usize, BOX);
                }
            }
        }
    }
    boxes
}

#[allow(clippy::too_many_arguments)]
fn is_possible_push_into_corral(
    level: &Level,
    distances: &Distances,
    board: &Board,
    pusher_y: usize,
    pusher_x: usize,
    dest_y: usize,
    dest_x: usize,
    cd: &CorralData,
) -> bool {
    if board.cell(pusher_y, pusher_x) == WALL {
        return false;
    }
    if board.cell(dest_y, dest_x) & OCCUPIED != 0 {
        return false;
    }
    if cd.corral.cell(pusher_y, pusher_x) != 0 {
        return false;
    }
    if cd.corral.cell(dest_y, dest_x) == 0 {
        return false;
    }

    let Some((box_y, box_x, direction)) =
        get_direction_from_pusher_dest(pusher_y, pusher_x, dest_y, dest_x)
    else {
        return false;
    };
    !deadlock_in_direction(level, distances, board, box_y, box_x, direction)
}

#[allow(clippy::too_many_arguments)]
fn is_possible_push_aside_corral(
    level: &Level,
    distances: &Distances,
    board: &Board,
    pusher_y: usize,
    pusher_x: usize,
    dest_y: usize,
    dest_x: usize,
    cd: &CorralData,
) -> bool {
    if board.cell(pusher_y, pusher_x) & OCCUPIED != 0 {
        return false;
    }
    if board.cell(dest_y, dest_x) & OCCUPIED != 0 {
        return false;
    }
    if cd.corral.cell(pusher_y, pusher_x) != 0 {
        return false;
    }
    if cd.corral.cell(dest_y, dest_x) != 0 {
        return false;
    }

    let Some((box_y, box_x, direction)) =
        get_direction_from_pusher_dest(pusher_y, pusher_x, dest_y, dest_x)
    else {
        return false;
    };
    !deadlock_in_direction(level, distances, board, box_y, box_x, direction)
}

fn is_corral_active(board: &Board, cd: &CorralData) -> bool {
    let corral_boxes = get_corral_boxes(board, cd);
    for y in 0..board.height() {
        for x in 0..board.width() {
            if cd.corral.cell(y, x) != 0 && corral_boxes.cell(y, x) == (TARGET | SOKOBAN) {
                return true;
            }
            // `corral_boxes` marks the box ring around the corral; C++ checks
            // the whole board here, not only corral cells.
            if corral_boxes.cell(y, x) == BOX {
                return true;
            }
        }
    }
    false
}

fn set_corral_options(
    level: &Level,
    distances: &Distances,
    board_in: &Board,
    options: &mut Board,
    cd: &CorralData,
) {
    let corral_boxes = get_corral_boxes(board_in, cd);
    *options = *board_in;
    options.clear_boxes_inplace();

    for y in 0..board_in.height() {
        for x in 0..board_in.width() {
            if corral_boxes.cell(y, x) & BOX == 0 {
                continue;
            }
            for dir in 0..4 {
                let pusher_y = y as isize + DELTA_Y[dir];
                let pusher_x = x as isize + DELTA_X[dir];
                let dest_y = y as isize - DELTA_Y[dir];
                let dest_x = x as isize - DELTA_X[dir];
                let (pusher_y, pusher_x, dest_y, dest_x) = match (
                    usize::try_from(pusher_y),
                    usize::try_from(pusher_x),
                    usize::try_from(dest_y),
                    usize::try_from(dest_x),
                ) {
                    (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
                    _ => continue,
                };
                if !board_in.in_bounds(pusher_y, pusher_x) || !board_in.in_bounds(dest_y, dest_x) {
                    continue;
                }
                if is_possible_push_into_corral(
                    level,
                    distances,
                    &corral_boxes,
                    pusher_y,
                    pusher_x,
                    dest_y,
                    dest_x,
                    cd,
                ) && board_in.cell(pusher_y, pusher_x) & SOKOBAN != 0
                {
                    options.or(y, x, BOX);
                }
            }
        }
    }
}

fn get_corral_candidates(level: &Level, board: &Board, cd: &mut CorralData) {
    cd.corral_comps_num = crate::bfs::mark_connectivities(level, board, &mut cd.corral_d);
    if cd.corral_comps_num >= MAX_CORRALS_NUM {
        return;
    }

    let Some((sok_y, sok_x)) = board.sokoban_position() else {
        cd.corral_sokoban_comp = INF;
        return;
    };
    cd.corral_sokoban_comp = cd.corral_d[Board::flat_index(sok_y, sok_x)];
    cd.corral_candidates = [false; MAX_CORRALS_NUM];

    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            for dir in 0..4 {
                let ny = y as isize + DELTA_Y[dir];
                let nx = x as isize + DELTA_X[dir];
                if ny < 0 || nx < 0 {
                    continue;
                }
                let (ny, nx) = (ny as usize, nx as usize);
                if !board.in_bounds(ny, nx) {
                    continue;
                }
                if cd.corral_d[Board::flat_index(ny, nx)] != cd.corral_sokoban_comp {
                    continue;
                }
                let oy = y as isize - DELTA_Y[dir];
                let ox = x as isize - DELTA_X[dir];
                if oy < 0 || ox < 0 {
                    continue;
                }
                let (oy, ox) = (oy as usize, ox as usize);
                if !board.in_bounds(oy, ox) {
                    continue;
                }
                let other = cd.corral_d[Board::flat_index(oy, ox)];
                if other != INF
                    && other != cd.corral_sokoban_comp
                    && (other as usize) < MAX_CORRALS_NUM
                {
                    cd.corral_candidates[other as usize] = true;
                }
            }
        }
    }
}

fn init_corral_with_comp(comp1: usize, comp2: usize, board: &Board, cd: &mut CorralData) -> bool {
    cd.corral.zero();
    cd.corral_component_used = [false; MAX_CORRALS_NUM];
    cd.corral_component_used[comp1] = true;
    cd.corral_component_used[comp2] = true;

    for y in 0..board.height() {
        for x in 0..board.width() {
            let value = cd.corral_d[Board::flat_index(y, x)];
            if value == comp1 as u32 || value == comp2 as u32 {
                cd.corral.set(y, x, 1);
            }
        }
    }
    is_corral_active(board, cd)
}

fn update_corral_with_comp(comp: usize, board: &Board, cd: &mut CorralData) {
    cd.corral_component_used[comp] = true;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if cd.corral_d[Board::flat_index(y, x)] == comp as u32 {
                cd.corral.set(y, x, 1);
            }
        }
    }
}

fn box_touches_corral_and_non_corral(board: &Board, y: usize, x: usize, cd: &CorralData) -> bool {
    let mut touches_corral = false;
    let mut touches_non_corral = false;
    for dir in 0..4 {
        let ny = y as isize + DELTA_Y[dir];
        let nx = x as isize + DELTA_X[dir];
        if cd.corral.get(ny, nx) != 0 {
            touches_corral = true;
        }
        if board.get(ny, nx) & OCCUPIED == 0 && cd.corral.get(ny, nx) == 0 {
            touches_non_corral = true;
        }
    }
    touches_corral && touches_non_corral
}

fn should_add_component_to_corral(
    level: &Level,
    distances: &Distances,
    corral_boxes: &Board,
    cd: &mut CorralData,
) -> Option<usize> {
    for y in 0..corral_boxes.height() {
        for x in 0..corral_boxes.width() {
            if corral_boxes.cell(y, x) & BOX == 0 {
                continue;
            }
            if !box_touches_corral_and_non_corral(corral_boxes, y, x, cd) {
                continue;
            }

            for k in 0..4 {
                let pusher_y = y as isize + DELTA_Y[k];
                let pusher_x = x as isize + DELTA_X[k];
                let dest_y = y as isize - DELTA_Y[k];
                let dest_x = x as isize - DELTA_X[k];
                let (pusher_y, pusher_x, dest_y, dest_x) = match (
                    usize::try_from(pusher_y),
                    usize::try_from(pusher_x),
                    usize::try_from(dest_y),
                    usize::try_from(dest_x),
                ) {
                    (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
                    _ => continue,
                };
                if !corral_boxes.in_bounds(pusher_y, pusher_x)
                    || !corral_boxes.in_bounds(dest_y, dest_x)
                {
                    continue;
                }

                let mut can_push_inside = is_possible_push_into_corral(
                    level,
                    distances,
                    corral_boxes,
                    pusher_y,
                    pusher_x,
                    dest_y,
                    dest_x,
                    cd,
                );
                if corral_boxes.cell(pusher_y, pusher_x) & OCCUPIED != 0 {
                    can_push_inside = false;
                }

                let can_push_aside = is_possible_push_aside_corral(
                    level,
                    distances,
                    corral_boxes,
                    pusher_y,
                    pusher_x,
                    dest_y,
                    dest_x,
                    cd,
                );

                if !can_push_inside && !can_push_aside {
                    continue;
                }

                if can_push_inside {
                    let d = cd.corral_d[Board::flat_index(pusher_y, pusher_x)];
                    if d != INF
                        && d != cd.corral_sokoban_comp
                        && (d as usize) < MAX_CORRALS_NUM
                        && !cd.corral_component_used[d as usize]
                    {
                        return Some(d as usize);
                    }
                }

                if can_push_aside {
                    let d = cd.corral_d[Board::flat_index(dest_y, dest_x)];
                    if d == INF || (d as usize) >= MAX_CORRALS_NUM {
                        continue;
                    }
                    if d != cd.corral_sokoban_comp && !cd.corral_component_used[d as usize] {
                        return Some(d as usize);
                    }
                }
            }
        }
    }
    None
}

fn can_push_box_out_of_corral(
    level: &Level,
    distances: &Distances,
    board: &Board,
    corral: &Board,
) -> bool {
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            if !level.is_inner(y, x) {
                continue;
            }
            for k in 0..4 {
                let pusher_y = y as isize + DELTA_Y[k];
                let pusher_x = x as isize + DELTA_X[k];
                let dest_y = y as isize - DELTA_Y[k];
                let dest_x = x as isize - DELTA_X[k];
                let (pusher_y, pusher_x, dest_y, dest_x) = match (
                    usize::try_from(pusher_y),
                    usize::try_from(pusher_x),
                    usize::try_from(dest_y),
                    usize::try_from(dest_x),
                ) {
                    (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
                    _ => continue,
                };
                if board.cell(pusher_y, pusher_x) & OCCUPIED != 0 {
                    continue;
                }
                if board.cell(dest_y, dest_x) & OCCUPIED != 0 {
                    continue;
                }
                if deadlock_in_direction(level, distances, board, y, x, k) {
                    continue;
                }
                if corral.cell(pusher_y, pusher_x) == 0 && corral.cell(dest_y, dest_x) == 0 {
                    return true;
                }
            }
        }
    }
    false
}

fn check_pi_corral(
    level: &Level,
    distances: &Distances,
    corral_boxes: &Board,
    board_in: &Board,
    cd: &CorralData,
) -> bool {
    for y in 0..corral_boxes.height() {
        for x in 0..corral_boxes.width() {
            if corral_boxes.cell(y, x) & BOX == 0 {
                continue;
            }
            for k in 0..4 {
                let pusher_y = y as isize + DELTA_Y[k];
                let pusher_x = x as isize + DELTA_X[k];
                let dest_y = y as isize - DELTA_Y[k];
                let dest_x = x as isize - DELTA_X[k];
                let (pusher_y, pusher_x, dest_y, dest_x) = match (
                    usize::try_from(pusher_y),
                    usize::try_from(pusher_x),
                    usize::try_from(dest_y),
                    usize::try_from(dest_x),
                ) {
                    (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
                    _ => continue,
                };
                if !corral_boxes.in_bounds(pusher_y, pusher_x)
                    || !corral_boxes.in_bounds(dest_y, dest_x)
                {
                    continue;
                }
                if !is_possible_push_into_corral(
                    level,
                    distances,
                    corral_boxes,
                    pusher_y,
                    pusher_x,
                    dest_y,
                    dest_x,
                    cd,
                ) {
                    continue;
                }
                if corral_boxes.cell(pusher_y, pusher_x) & OCCUPIED != 0 {
                    continue;
                }
                if cd.corral.cell(dest_y, dest_x) == 0 || cd.corral.cell(pusher_y, pusher_x) != 0 {
                    continue;
                }
                if board_in.cell(pusher_y, pusher_x) & SOKOBAN == 0 {
                    return false;
                }
            }
        }
    }
    true
}

fn expand_corral(
    level: &Level,
    distances: &Distances,
    board_in: &Board,
    cd: &mut CorralData,
) -> bool {
    let mut corral_boxes = get_corral_boxes(board_in, cd);
    while let Some(comp) = should_add_component_to_corral(level, distances, &corral_boxes, cd) {
        update_corral_with_comp(comp, board_in, cd);
        corral_boxes = get_corral_boxes(board_in, cd);
    }

    if can_push_box_out_of_corral(level, distances, &corral_boxes, &cd.corral) {
        return false;
    }
    check_pi_corral(level, distances, &corral_boxes, board_in, cd)
}

fn touches_two_components(board: &Board, cd: &CorralData) -> Vec<Vec<bool>> {
    let mut touches = vec![vec![false; MAX_CORRALS_NUM]; MAX_CORRALS_NUM];
    if cd.corral_comps_num >= MAX_CORRALS_NUM {
        return touches;
    }

    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            for k in 0..4 {
                for l in 0..4 {
                    let d1y = y as isize + DELTA_Y[k];
                    let d1x = x as isize + DELTA_X[k];
                    let d2y = y as isize + DELTA_Y[l];
                    let d2x = x as isize + DELTA_X[l];
                    if d1y < 0 || d1x < 0 || d2y < 0 || d2x < 0 {
                        continue;
                    }
                    let (d1y, d1x) = (d1y as usize, d1x as usize);
                    let (d2y, d2x) = (d2y as usize, d2x as usize);
                    if !board.in_bounds(d1y, d1x) || !board.in_bounds(d2y, d2x) {
                        continue;
                    }
                    let d1 = cd.corral_d[Board::flat_index(d1y, d1x)];
                    let d2 = cd.corral_d[Board::flat_index(d2y, d2x)];
                    if d1 != INF
                        && d2 != INF
                        && (d1 as usize) < MAX_CORRALS_NUM
                        && (d2 as usize) < MAX_CORRALS_NUM
                    {
                        touches[d1 as usize][d2 as usize] = true;
                    }
                }
            }
        }
    }
    touches
}

/// Returns the corral options board (cells where pushing is possible) or
/// `None` when no active pi-corral was found.
pub fn detect_corral(level: &Level, distances: &Distances, board: &Board) -> Option<Board> {
    let mut cd = CorralData::new(level);
    get_corral_candidates(level, board, &mut cd);
    if cd.corral_comps_num >= MAX_CORRALS_NUM {
        return None;
    }

    let mut options = *board;

    for i in 0..cd.corral_comps_num {
        if !cd.corral_candidates[i] {
            continue;
        }
        if !init_corral_with_comp(i, i, board, &mut cd) {
            continue;
        }
        if expand_corral(level, distances, board, &mut cd) {
            set_corral_options(level, distances, board, &mut options, &cd);
            return Some(options);
        }
    }

    let touches = touches_two_components(board, &cd);

    for (i, row) in touches.iter().enumerate().take(cd.corral_comps_num) {
        if i as u32 == cd.corral_sokoban_comp {
            continue;
        }
        let remaining = cd.corral_comps_num.saturating_sub(i + 1);
        for (j, &touch) in row.iter().enumerate().skip(i + 1).take(remaining) {
            if j as u32 == cd.corral_sokoban_comp {
                continue;
            }
            if !cd.corral_candidates[j] && !cd.corral_candidates[i] {
                continue;
            }
            if !touch {
                continue;
            }
            if !init_corral_with_comp(i, j, board, &mut cd) {
                continue;
            }
            if expand_corral(level, distances, board, &mut cd) {
                set_corral_options(level, distances, board, &mut options, &cd);
                return Some(options);
            }
        }
    }

    None
}
