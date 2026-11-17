use crate::board::Board;
use crate::constants::*;
use crate::level::Level;

#[inline]
pub fn flat(y: usize, x: usize) -> usize {
    y * MAX_SIZE + x
}

/// Fills the logical rectangle with `INF`. Cells outside it are never read,
/// matching C++ `init_dist`, which only clears `height` rows of `width` cells.
pub fn init_dist(dist: &mut [u32], width: usize, height: usize) {
    for y in 0..height {
        let start = y * MAX_SIZE;
        dist[start..start + width].fill(INF);
    }
}

/// Breadth-first fill from one cell. `DISTANCES` selects whether the BFS depth
/// or the constant `mark` is written into every reached cell.
fn bfs_fill<const DISTANCES: bool>(
    board: &Board,
    dist: &mut [u32],
    start_y: usize,
    start_x: usize,
    ignore_boxes: bool,
    mark: u32,
) {
    debug_assert!(dist.len() >= MAX_INNER);
    let occupied_mask = if ignore_boxes { WALL } else { OCCUPIED };

    let mut queue = [0u16; MAX_INNER];
    queue[0] = flat(start_y, start_x) as u16;
    dist[flat(start_y, start_x)] = mark;
    let mut head = 0usize;
    let mut tail = 1usize;
    let mut d = 0u32;

    while head < tail {
        let level_end = tail;
        while head < level_end {
            let index = queue[head] as usize;
            head += 1;
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
                    dist[next_index] = if DISTANCES { d + 1 } else { mark };
                    queue[tail] = next_index as u16;
                    tail += 1;
                }
            }
        }
        d += 1;
    }
}

pub fn bfs_from_place(
    board: &Board,
    dist: &mut [u32],
    start_y: usize,
    start_x: usize,
    ignore_boxes: bool,
) {
    bfs_fill::<true>(board, dist, start_y, start_x, ignore_boxes, 0);
}

/// Marks one free component with a constant label (`mark_connectivities`).
fn bfs_mark(board: &Board, dist: &mut [u32], start_y: usize, start_x: usize, mark: u32) {
    bfs_fill::<false>(board, dist, start_y, start_x, false, mark);
}

pub fn bfs(board: &Board, dist: &mut [u32], start_y: usize, start_x: usize, ignore_boxes: bool) {
    init_dist(dist, board.width(), board.height());
    bfs_from_place(board, dist, start_y, start_x, ignore_boxes);
}

fn connectivity_with_buffers(
    level: &Level,
    board: &Board,
    visited: &mut [bool],
    queue: &mut [u16],
) -> usize {
    let height = board.height();
    let width = board.width();
    let cells = board.cells();
    let mut components = 0usize;

    for &(y, x) in &level.coords {
        let (y, x) = (y as usize, x as usize);
        if y >= height || x >= width {
            continue;
        }
        let start_idx = flat(y, x);
        if visited[start_idx] || cells[start_idx] & OCCUPIED != 0 {
            continue;
        }

        components += 1;
        visited[start_idx] = true;
        queue[0] = start_idx as u16;
        let mut head = 0usize;
        let mut tail = 1usize;

        while head < tail {
            let index = queue[head] as usize;
            head += 1;
            let cy = index / MAX_SIZE;
            let cx = index % MAX_SIZE;
            for dir in 0..4 {
                let ny = cy.wrapping_add_signed(DELTA_Y[dir]);
                let nx = cx.wrapping_add_signed(DELTA_X[dir]);
                if ny >= height || nx >= width {
                    continue;
                }
                let next_idx = flat(ny, nx);
                if !visited[next_idx] && cells[next_idx] & OCCUPIED == 0 {
                    visited[next_idx] = true;
                    queue[tail] = next_idx as u16;
                    tail += 1;
                }
            }
        }
    }

    components
}

pub fn get_connectivity(level: &Level, board: &Board) -> usize {
    const SMALL_ROWS: usize = 20;
    const SMALL_INNER: usize = 256;
    if board.height() <= SMALL_ROWS && level.index_num() <= SMALL_INNER {
        let mut visited = [false; SMALL_ROWS * MAX_SIZE];
        let mut queue = [0u16; SMALL_INNER];
        connectivity_with_buffers(level, board, &mut visited, &mut queue)
    } else {
        let mut visited = [false; MAX_INNER];
        let mut queue = [0u16; MAX_INNER];
        connectivity_with_buffers(level, board, &mut visited, &mut queue)
    }
}

pub fn mark_connectivities(level: &Level, board: &Board, dist: &mut [u32]) -> usize {
    let mut components = 0usize;
    init_dist(dist, board.width(), board.height());

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

            bfs_mark(board, dist, y, x, components as u32);
            components += 1;
        }
    }

    components
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
    init_dist(dist, board.width(), board.height());
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

    #[test]
    fn connectivity_matches_mark_connectivities() {
        let level = level(
            "############\n\
             #@  $   .  #\n\
             #  ### ##  #\n\
             #   $   .  #\n\
             ############\n",
        );
        let mut state = 0xfeed_face_cafe_babeu64;
        let mut dist = [INF; MAX_INNER];
        for _ in 0..300 {
            let mut b = level.board.without_boxes();
            b.clear_sokoban_inplace();
            for &(y, x) in &level.coords {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                if (state >> 61) == 0 {
                    b.or(y as usize, x as usize, BOX);
                }
            }
            assert_eq!(
                get_connectivity(&level, &b),
                mark_connectivities(&level, &b, &mut dist)
            );
        }
    }
}
