//! Biconnectivity scoring (`biconnect.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::level::Level;

fn low_get(low: &[u32], y: isize, x: isize) -> u32 {
    if y < 0 || x < 0 || y as usize >= MAX_SIZE || x as usize >= MAX_SIZE {
        // The C++ global int board is zero outside the initialized rectangle.
        return 0;
    }
    low[Board::flat_index(y as usize, x as usize)]
}

#[inline]
fn mark_cycle(on_cycle: &mut [u8], y: usize, x: usize, cycle_cells: &mut usize) {
    let idx = Board::flat_index(y, x);
    if on_cycle[idx] == 0 {
        on_cycle[idx] = 1;
        *cycle_cells += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn biconnect_dfs(
    b: &Board,
    y: usize,
    x: usize,
    father: i32,
    d: u32,
    visited: &mut [u8],
    low: &mut [u32],
    on_cycle: &mut [u8],
    cycle_cells: &mut usize,
) {
    let cur_idx = Board::flat_index(y, x);
    visited[cur_idx] = 1;
    low[cur_idx] = d;

    for dir in 0..4 {
        let ny = y as isize + DELTA_Y[dir];
        let nx = x as isize + DELTA_X[dir];
        if b.get(ny, nx) & OCCUPIED != 0 {
            continue;
        }
        if ny < 0 || nx < 0 {
            continue;
        }
        let (ny, nx) = (ny as usize, nx as usize);
        if !b.in_bounds(ny, nx) || visited[Board::flat_index(ny, nx)] != 0 {
            continue;
        }
        biconnect_dfs(
            b,
            ny,
            nx,
            ((dir + 2) & 3) as i32,
            d + 1,
            visited,
            low,
            on_cycle,
            cycle_cells,
        );
    }

    for dir in 0..4 {
        if dir as i32 == father {
            continue;
        }
        let ny = y as isize + DELTA_Y[dir];
        let nx = x as isize + DELTA_X[dir];
        let neighbor_low = low_get(low, ny, nx);
        if neighbor_low < low[cur_idx] {
            low[cur_idx] = neighbor_low;
        }
        if neighbor_low < d && ny >= 0 && nx >= 0 {
            let (ny, nx) = (ny as usize, nx as usize);
            if b.in_bounds(ny, nx) {
                mark_cycle(on_cycle, ny, nx, cycle_cells);
            }
        }
    }

    if low[cur_idx] < d {
        mark_cycle(on_cycle, y, x, cycle_cells);
    }
}

pub fn biconnect_score(level: &Level, b: &Board) -> usize {
    let mut visited = [0u8; MAX_INNER];
    let mut on_cycle = [0u8; MAX_INNER];
    let mut low = [INF; MAX_INNER];
    let mut cycle_cells = 0usize;

    for &(y, x) in &level.coords {
        let (y, x) = (y as usize, x as usize);
        if !b.in_bounds(y, x) {
            continue;
        }
        let idx = Board::flat_index(y, x);
        if visited[idx] != 0 || b.cells()[idx] & OCCUPIED != 0 {
            continue;
        }
        biconnect_dfs(
            b,
            y,
            x,
            -1,
            0,
            &mut visited,
            &mut low,
            &mut on_cycle,
            &mut cycle_cells,
        );
    }

    for y in 0..b.height() {
        let row = y * MAX_SIZE;
        for x in 0..b.width() {
            let cell = b.cells()[row + x];
            if cell & (TARGET | BOX) == TARGET {
                mark_cycle(&mut on_cycle, y, x, &mut cycle_cells);
            }
        }
    }

    level.index_num() - level.box_count - cycle_cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    #[test]
    fn score_is_within_bounds() {
        let level =
            Level::preprocess(parse_levels("#######\n#@ $ .#\n#######\n").remove(0)).unwrap();
        let score = biconnect_score(&level, &level.board);
        assert!(score <= level.index_num());
    }
}
