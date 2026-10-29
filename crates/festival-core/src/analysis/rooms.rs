//! Room decomposition (`rooms.cpp`).

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::level::Level;

pub struct Rooms {
    pub board: Vec<u32>,
    pub num: usize,
    pub base_graph: [[bool; MAX_ROOMS]; MAX_ROOMS],
}

impl Rooms {
    #[inline]
    pub fn color(&self, y: usize, x: usize) -> u32 {
        self.board[Board::flat_index(y, x)]
    }

    pub fn room_bfs(
        &self,
        b: &Board,
        start_y: usize,
        start_x: usize,
        color: usize,
        seen_during_bfs: &mut [bool; MAX_ROOMS],
        visited: &mut Board,
    ) {
        let mut bfs_visited = Board::new(b.width(), b.height()).expect("same dims");
        seen_during_bfs.fill(false);

        let mut current = [0u16; MAX_INNER];
        let mut next = [0u16; MAX_INNER];
        let mut cur: &mut [u16] = &mut current;
        let mut nxt: &mut [u16] = &mut next;
        let mut current_len = 1usize;

        cur[0] = Board::flat_index(start_y, start_x) as u16;
        visited.set(start_y, start_x, 1);
        bfs_visited.set(start_y, start_x, 1);

        while current_len > 0 {
            let mut next_len = 0usize;
            for &index in &cur[..current_len] {
                let y = index as usize / MAX_SIZE;
                let x = index as usize % MAX_SIZE;
                for dir in 0..4 {
                    let ny = y as isize + DELTA_Y[dir];
                    let nx = x as isize + DELTA_X[dir];
                    if ny < 0 || nx < 0 {
                        continue;
                    }
                    let (ny, nx) = (ny as usize, nx as usize);
                    if !b.in_bounds(ny, nx) {
                        continue;
                    }

                    let seen_color = self.board[Board::flat_index(ny, nx)];
                    if seen_color != INF && seen_color != color as u32 {
                        let idx = seen_color as usize;
                        if idx < MAX_ROOMS {
                            seen_during_bfs[idx] = true;
                        }
                        continue;
                    }

                    if bfs_visited.cell(ny, nx) != 0 {
                        continue;
                    }
                    if visited.cell(ny, nx) != 0 {
                        continue;
                    }
                    if b.cell(ny, nx) & OCCUPIED != 0 {
                        continue;
                    }

                    bfs_visited.set(ny, nx, 1);
                    if seen_color == color as u32 {
                        visited.set(ny, nx, 1);
                    }
                    nxt[next_len] = Board::flat_index(ny, nx) as u16;
                    next_len += 1;
                }
            }
            std::mem::swap(&mut cur, &mut nxt);
            current_len = next_len;
        }
    }

    pub fn current_graph(&self, level: &Level, b: &Board) -> [[bool; MAX_ROOMS]; MAX_ROOMS] {
        let mut graph = [[true; MAX_ROOMS]; MAX_ROOMS];
        for (i, row) in graph.iter_mut().enumerate() {
            row[i] = false;
        }

        let mut visited = Board::new(b.width(), b.height()).expect("same dims");
        let mut seen = [false; MAX_ROOMS];
        let mut color_processed = [false; MAX_ROOMS];

        for y in 0..b.height() {
            for x in 0..b.width() {
                if !level.is_inner(y, x) {
                    continue;
                }
                if b.cell(y, x) & OCCUPIED != 0 {
                    continue;
                }
                if visited.cell(y, x) != 0 {
                    continue;
                }
                let color = self.color(y, x);
                if color == INF {
                    continue;
                }
                let color = color as usize;
                color_processed[color] = true;

                self.room_bfs(b, y, x, color, &mut seen, &mut visited);

                for (k, was_seen) in seen.iter().enumerate().take(self.num) {
                    if !was_seen {
                        graph[color][k] = false;
                        graph[k][color] = false;
                    }
                }
            }
        }

        for (i, processed) in color_processed.iter().enumerate().take(self.num) {
            if !processed {
                graph[i].fill(false);
                for row in graph.iter_mut().take(self.num) {
                    row[i] = false;
                }
            }
        }

        graph
    }

    pub fn score(&self, level: &Level, board: &Board) -> usize {
        if self.num <= 1 {
            return 0;
        }
        let graph = self.current_graph(level, board);
        let mut sum = 0i32;
        for (base_row, graph_row) in self.base_graph.iter().zip(graph.iter()).take(self.num) {
            for (base, current) in base_row.iter().zip(graph_row.iter()).take(self.num) {
                sum += *base as i32 - *current as i32;
            }
        }
        debug_assert!(sum % 2 == 0, "C++ exits on odd room score");
        (sum / 2).max(0) as usize
    }
}

pub fn analyse(level: &Level, board: &Board) -> Result<Rooms> {
    let mut b = *board;
    b.clear_boxes_inplace();
    b.clear_sokoban_inplace();
    b.clear_targets_inplace();

    let (rooms_board, num) = new_mark_rooms(level, &b)?;
    if num > MAX_ROOMS {
        return Err(SolveError::TooManyRooms);
    }

    let c = b;
    let mut rooms = Rooms {
        board: rooms_board,
        num,
        base_graph: [[false; MAX_ROOMS]; MAX_ROOMS],
    };

    rooms.base_graph = rooms.current_graph(level, &c);
    Ok(rooms)
}

fn new_mark_rooms(level: &Level, b: &Board) -> Result<(Vec<u32>, usize)> {
    let height = b.height();
    let width = b.width();
    let mut squares = Board::new(width, height)?;

    for i in 0..height.saturating_sub(1) {
        for j in 0..width.saturating_sub(1) {
            if b.cell(i, j) == 0
                && b.cell(i, j + 1) == 0
                && b.cell(i + 1, j) == 0
                && b.cell(i + 1, j + 1) == 0
            {
                squares.set(i, j, 1);
            }
        }
    }

    for i in 0..height {
        for j in 0..width {
            if squares.cell(i, j) == 1
                && squares.get_u(i.wrapping_sub(1), j) == 0
                && squares.get_u(i, j + 1) == 0
                && squares.get_u(i + 1, j) == 0
                && squares.get_u(i, j.wrapping_sub(1)) == 0
            {
                squares.set(i, j, 0);
            }
        }
    }

    squares.negate()?;

    let mut tmp = vec![INF; MAX_INNER];
    let num = bfs::mark_connectivities(level, &squares, &mut tmp);

    let mut rooms_board = vec![INF; MAX_INNER];
    for i in 0..height {
        for j in 0..width {
            let v = tmp[Board::flat_index(i, j)];
            if v == INF {
                continue;
            }
            rooms_board[Board::flat_index(i, j)] = v;
            if j + 1 < width {
                rooms_board[Board::flat_index(i, j + 1)] = v;
            }
            if i + 1 < height {
                rooms_board[Board::flat_index(i + 1, j)] = v;
                if j + 1 < width {
                    rooms_board[Board::flat_index(i + 1, j + 1)] = v;
                }
            }
        }
    }

    for i in 0..height.saturating_sub(1) {
        for j in 0..width.saturating_sub(1) {
            if squares.cell(i, j) == squares.cell(i + 1, j + 1)
                && squares.cell(i + 1, j) == squares.cell(i, j + 1)
                && squares.cell(i, j) != squares.cell(i + 1, j)
            {
                rooms_board[Board::flat_index(i + 1, j + 1)] = INF;
            }
        }
    }

    Ok((rooms_board, num))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn open_room_is_detected() {
        let level = level("########\n#      #\n#  @   #\n#      #\n########\n");
        let rooms = analyse(&level, &level.board).unwrap();
        assert!(rooms.num >= 1);
    }

    #[test]
    fn corridor_has_no_rooms() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let rooms = analyse(&level, &level.board).unwrap();
        assert_eq!(rooms.num, 0);
    }
}
