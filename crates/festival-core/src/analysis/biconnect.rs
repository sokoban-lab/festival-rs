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

#[allow(clippy::too_many_arguments)]
fn biconnect_dfs(
    b: &Board,
    y: usize,
    x: usize,
    father: i32,
    d: u32,
    visited: &mut Board,
    low: &mut [u32],
    on_cycle: &mut Board,
) {
    visited.set(y, x, 1);
    low[Board::flat_index(y, x)] = d;

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
        if !b.in_bounds(ny, nx) || visited.cell(ny, nx) != 0 {
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
        );
    }

    for dir in 0..4 {
        if dir as i32 == father {
            continue;
        }
        let ny = y as isize + DELTA_Y[dir];
        let nx = x as isize + DELTA_X[dir];
        let neighbor_low = low_get(low, ny, nx);
        if neighbor_low < low[Board::flat_index(y, x)] {
            low[Board::flat_index(y, x)] = neighbor_low;
        }
        if neighbor_low < d && ny >= 0 && nx >= 0 {
            on_cycle.or(ny as usize, nx as usize, 1);
        }
    }

    if low[Board::flat_index(y, x)] < d {
        on_cycle.set(y, x, 1);
    }
}

pub fn biconnect_score(level: &Level, board_in: &Board) -> usize {
    let b = *board_in;
    let mut visited = Board::new(b.width(), b.height()).expect("same dims");
    let mut on_cycle = Board::new(b.width(), b.height()).expect("same dims");
    let mut low = vec![INF; MAX_INNER];

    for y in 0..b.height() {
        for x in 0..b.width() {
            if !level.is_inner(y, x) {
                continue;
            }
            if visited.cell(y, x) != 0 || b.cell(y, x) & OCCUPIED != 0 {
                continue;
            }
            biconnect_dfs(&b, y, x, -1, 0, &mut visited, &mut low, &mut on_cycle);
        }
    }

    for y in 0..b.height() {
        for x in 0..b.width() {
            if b.cell(y, x) & TARGET != 0 && b.cell(y, x) & BOX == 0 {
                on_cycle.set(y, x, 1);
            }
        }
    }

    let cycle_cells = on_cycle
        .cells()
        .iter()
        .take(MAX_INNER)
        .filter(|cell| **cell != 0)
        .count();
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
