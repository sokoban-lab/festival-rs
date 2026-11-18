//! Room decomposition (`rooms.cpp`).

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::level::Level;

const _: () = assert!(MAX_ROOMS <= 128);
const PERM_VISITED: u16 = u16::MAX;
const SMALL_ROWS: usize = 20;
const SMALL_INNER: usize = 256;

pub struct Rooms {
    pub board: Vec<u32>,
    pub num: usize,
    pub base_graph: [[bool; MAX_ROOMS]; MAX_ROOMS],
    base_masks: [u128; MAX_ROOMS],
    base_edges: i32,
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

    fn current_masks_with_buffers(
        &self,
        level: &Level,
        b: &Board,
        stamp: &mut [u16],
        queue: &mut [u16],
    ) -> [u128; MAX_ROOMS] {
        let num = self.num;
        let all_rooms_mask = (1u128 << num) - 1;
        let mut masks = [0u128; MAX_ROOMS];
        for (i, slot) in masks.iter_mut().enumerate().take(num) {
            *slot = all_rooms_mask & !(1u128 << i);
        }

        let height = b.height();
        let width = b.width();
        let cells = b.cells();
        let rooms_board = &self.board[..MAX_INNER];
        let mut color_processed_mask = 0u128;
        let mut bfs_id = 0u16;

        for &(y, x) in &level.coords {
            let (y, x) = (y as usize, x as usize);
            if y >= height || x >= width {
                continue;
            }
            let start_flat = y * MAX_SIZE + x;
            if cells[start_flat] & OCCUPIED != 0 {
                continue;
            }
            if stamp[start_flat] == PERM_VISITED {
                continue;
            }
            let color_u32 = rooms_board[start_flat];
            if color_u32 == INF {
                continue;
            }
            let color = color_u32 as usize;
            color_processed_mask |= 1u128 << color;

            bfs_id += 1;
            let mut seen_mask = 0u128;
            stamp[start_flat] = PERM_VISITED;
            queue[0] = start_flat as u16;
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
                    let next_flat = ny * MAX_SIZE + nx;
                    let seen_color = rooms_board[next_flat];
                    if seen_color != INF && seen_color != color_u32 {
                        let idx = seen_color as usize;
                        if idx < MAX_ROOMS {
                            seen_mask |= 1u128 << idx;
                        }
                        continue;
                    }

                    let s = stamp[next_flat];
                    if s == bfs_id || s == PERM_VISITED {
                        continue;
                    }
                    if cells[next_flat] & OCCUPIED != 0 {
                        continue;
                    }

                    stamp[next_flat] = if seen_color == color_u32 {
                        PERM_VISITED
                    } else {
                        bfs_id
                    };
                    queue[tail] = next_flat as u16;
                    tail += 1;
                }
            }

            masks[color] &= seen_mask;
            let mut unseen = (!seen_mask) & all_rooms_mask;
            let clear_color = !(1u128 << color);
            while unseen != 0 {
                let k = unseen.trailing_zeros() as usize;
                masks[k] &= clear_color;
                unseen &= unseen - 1;
            }
        }

        for (i, slot) in masks.iter_mut().enumerate().take(num) {
            if (color_processed_mask >> i) & 1 == 0 {
                *slot = 0;
            } else {
                *slot &= color_processed_mask;
            }
        }

        masks
    }

    #[inline]
    fn current_masks(&self, level: &Level, b: &Board) -> [u128; MAX_ROOMS] {
        if b.height() <= SMALL_ROWS && level.index_num() <= SMALL_INNER {
            let mut stamp = [0u16; SMALL_ROWS * MAX_SIZE];
            let mut queue = [0u16; SMALL_INNER];
            self.current_masks_with_buffers(level, b, &mut stamp, &mut queue)
        } else {
            let mut stamp = [0u16; MAX_INNER];
            let mut queue = [0u16; MAX_INNER];
            self.current_masks_with_buffers(level, b, &mut stamp, &mut queue)
        }
    }

    pub fn score(&self, level: &Level, board: &Board) -> usize {
        if self.num <= 1 {
            return 0;
        }
        let cur_masks = self.current_masks(level, board);
        let mut cur_edges = 0i32;
        for &mask in cur_masks.iter().take(self.num) {
            cur_edges += mask.count_ones() as i32;
        }
        let sum = self.base_edges - cur_edges;
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
        base_masks: [0; MAX_ROOMS],
        base_edges: 0,
    };

    rooms.base_graph = rooms.current_graph(level, &c);
    let mut base_edges = 0i32;
    for (i, row) in rooms.base_graph.iter().enumerate().take(num) {
        let mut mask = 0u128;
        for (j, &connected) in row.iter().enumerate().take(num) {
            if connected {
                mask |= 1u128 << j;
                base_edges += 1;
            }
        }
        rooms.base_masks[i] = mask;
    }
    rooms.base_edges = base_edges;
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

    #[test]
    fn fast_room_score_matches_reference_current_graph() {
        let level = level(
            "###############\n\
             #    #    #   #\n\
             # @  $    $ . #\n\
             ### #### ### ##\n\
             #    #    #   #\n\
             #    $    .   #\n\
             #    #    #   #\n\
             ###############\n",
        );
        let rooms = analyse(&level, &level.board).unwrap();
        assert!(rooms.num >= 4);

        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..500 {
            let mut b = level.board.without_boxes();
            b.clear_sokoban_inplace();
            for &(y, x) in &level.coords {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                if (state >> 61) == 0 {
                    b.or(y as usize, x as usize, BOX);
                }
            }
            let graph = rooms.current_graph(&level, &b);
            let mut sum = 0i32;
            for (base_row, graph_row) in rooms.base_graph.iter().zip(graph.iter()).take(rooms.num) {
                for (base, current) in base_row.iter().zip(graph_row.iter()).take(rooms.num) {
                    sum += *base as i32 - *current as i32;
                }
            }
            let expected = (sum / 2).max(0) as usize;
            assert_eq!(rooms.score(&level, &b), expected);
        }
    }
}
