// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

use crate::board::Board;
use crate::constants::*;
use crate::level::Level;

#[inline]
pub fn flat(y: usize, x: usize) -> usize {
    y * MAX_SIZE + x
}

pub fn init_dist(dist: &mut [u32]) {
    dist.fill(INF);
}

pub fn bfs_from_place(
    board: &Board,
    dist: &mut [u32],
    start_y: usize,
    start_x: usize,
    ignore_boxes: bool,
) {
    debug_assert!(dist.len() >= MAX_INNER);
    let occupied_mask = if ignore_boxes { WALL } else { OCCUPIED };

    let mut current: Vec<usize> = Vec::with_capacity(MAX_INNER);
    let mut next: Vec<usize> = Vec::with_capacity(MAX_INNER);

    current.push(flat(start_y, start_x));
    dist[flat(start_y, start_x)] = 0;
    let mut d = 0u32;

    while !current.is_empty() {
        next.clear();
        for &index in &current {
            let y = index / MAX_SIZE;
            let x = index % MAX_SIZE;
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
                let next_index = flat(ny, nx);
                if board.cell(ny, nx) & occupied_mask == 0 && dist[next_index] == INF {
                    dist[next_index] = d + 1;
                    next.push(next_index);
                }
            }
        }
        d += 1;
        std::mem::swap(&mut current, &mut next);
    }
}

pub fn bfs(board: &Board, dist: &mut [u32], start_y: usize, start_x: usize, ignore_boxes: bool) {
    init_dist(dist);
    bfs_from_place(board, dist, start_y, start_x, ignore_boxes);
}

pub fn get_connectivity(level: &Level, board: &Board) -> usize {
    let mut dist = [INF; MAX_INNER];
    let mut components = 0;

    for y in 0..board.height() {
        for x in 0..board.width() {
            let idx = flat(y, x);
            if !level.is_inner(y, x) {
                continue;
            }
            if dist[idx] != INF {
                continue;
            }
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }
            bfs_from_place(board, &mut dist, y, x, false);
            components += 1;
        }
    }

    components
}

pub fn mark_connectivities(level: &Level, board: &Board, dist: &mut [u32]) -> usize {
    let mut local = [INF; MAX_INNER];
    let mut components = 0usize;
    init_dist(dist);

    for y in 0..board.height() {
        for x in 0..board.width() {
            let idx = flat(y, x);
            if !level.is_inner(y, x) {
                continue;
            }
            if dist[idx] != INF {
                continue;
            }
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }

            init_dist(&mut local);
            bfs_from_place(board, &mut local, y, x, false);

            for yy in 0..board.height() {
                for xx in 0..board.width() {
                    let local_idx = flat(yy, xx);
                    if local[local_idx] < INF {
                        dist[local_idx] = components as u32;
                    }
                }
            }
            components += 1;
        }
    }

    components
}

pub fn mark_connectivities_around_place(
    level: &Level,
    board: &Board,
    dist: &mut [u32],
    start_y: usize,
    start_x: usize,
) {
    let mut local = [INF; MAX_INNER];
    init_dist(dist);

    for color in 0..4usize {
        let y = start_y as isize + DELTA_Y[color];
        let x = start_x as isize + DELTA_X[color];
        if y < 0 || x < 0 {
            continue;
        }
        let (y, x) = (y as usize, x as usize);
        if !board.in_bounds(y, x) || !level.is_inner(y, x) {
            continue;
        }
        let idx = flat(y, x);
        if dist[idx] != INF {
            continue;
        }
        if board.cell(y, x) & OCCUPIED != 0 {
            continue;
        }

        init_dist(&mut local);
        bfs_from_place(board, &mut local, y, x, false);

        for yy in 0..board.height() {
            for xx in 0..board.width() {
                let local_idx = flat(yy, xx);
                if local[local_idx] < INF {
                    dist[local_idx] = color as u32;
                }
            }
        }
    }
}

pub fn get_connectivity_sizes(dist: &[u32], n: usize, sizes: &mut [usize]) {
    sizes[..n].fill(0);
    for &value in dist.iter().take(MAX_INNER) {
        if value != INF {
            sizes[value as usize] += 1;
        }
    }
}

/// Marks positions from which a box can be pulled to any target.
pub fn get_srcs_of_all_targets(level: &Level, board: &Board) -> Board {
    let mut srcs = Board::new(board.width(), board.height()).expect("same dims");
    let b = board.without_boxes();
    let mut queue: Vec<usize> = Vec::with_capacity(MAX_INNER);

    for y in 0..b.height() {
        for x in 0..b.width() {
            if !level.is_inner(y, x) {
                continue;
            }
            if b.cell(y, x) & TARGET == 0 {
                continue;
            }
            if let Some(index) = level.index_at(y, x) {
                queue.push(index);
                srcs.set(y, x, 1);
            }
        }
    }

    let mut pos = 0;
    while pos < queue.len() {
        let (y, x) = level.coord(queue[pos]);
        pos += 1;

        for dir in 0..4 {
            let box_y = y as isize + DELTA_Y[dir];
            let box_x = x as isize + DELTA_X[dir];
            if box_y < 0 || box_x < 0 {
                continue;
            }
            let (box_y, box_x) = (box_y as usize, box_x as usize);
            if !b.in_bounds(box_y, box_x) {
                continue;
            }
            if b.cell(box_y, box_x) & OCCUPIED != 0 {
                continue;
            }
            let player_y = box_y as isize + DELTA_Y[dir];
            let player_x = box_x as isize + DELTA_X[dir];
            if b.get(player_y, player_x) & OCCUPIED != 0 {
                continue;
            }
            if srcs.cell(box_y, box_x) != 0 {
                continue;
            }
            let Some(index) = level.index_at(box_y, box_x) else {
                continue;
            };
            srcs.set(box_y, box_x, 1);
            queue.push(index);
        }
    }

    srcs
}

/// Expands a group of cells through free cells, recording distances in `dist`.
pub fn expand_group(level: &Level, board: &Board, group: &mut Board, dist: &mut [u32]) {
    init_dist(dist);
    let mut current: Vec<usize> = Vec::with_capacity(MAX_INNER);
    let mut next: Vec<usize> = Vec::with_capacity(MAX_INNER);

    for y in 0..group.height() {
        for x in 0..group.width() {
            if group.cell(y, x) != 0 {
                if let Some(index) = level.index_at(y, x) {
                    current.push(index);
                }
                dist[flat(y, x)] = 0;
            }
        }
    }

    while !current.is_empty() {
        next.clear();
        for &index in &current {
            let (y, x) = level.coord(index);
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
                if group.cell(ny, nx) != 0 {
                    continue;
                }
                if board.cell(ny, nx) & OCCUPIED != 0 {
                    continue;
                }
                group.set(ny, nx, 1);
                if let Some(next_index) = level.index_at(ny, nx) {
                    next.push(next_index);
                }
                dist[flat(ny, nx)] = dist[flat(y, x)] + 1;
            }
        }
        std::mem::swap(&mut current, &mut next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn bfs_distances_in_open_room() {
        let level = level("#######\n#@    #\n#     #\n#######\n");
        let board = level.board.without_boxes();
        let mut dist = [INF; MAX_INNER];
        bfs(&board, &mut dist, 1, 1, true);
        assert_eq!(dist[flat(1, 1)], 0);
        assert_eq!(dist[flat(1, 5)], 4);
        assert_eq!(dist[flat(0, 0)], INF);
    }

    #[test]
    fn connectivity_counts_components() {
        let level = level("#######\n#@ $ .#\n#######\n");
        assert_eq!(get_connectivity(&level, &level.board), 2);
    }
}
