// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

use crate::constants::*;
use crate::error::{Result, SolveError};

/// Fixed-size Sokoban board.
///
/// The C++ implementation uses `unsigned char board[51][51]` and only reads
/// the first `height` rows / `width` columns. `get` reproduces that layout by
/// returning `0` outside the logical rectangle, so callers can keep the same
/// neighbor arithmetic without panicking.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Board {
    width: usize,
    height: usize,
    cells: [u8; MAX_INNER],
}

impl std::fmt::Debug for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display())
    }
}

impl Board {
    pub fn new(width: usize, height: usize) -> Result<Self> {
        if width == 0 || height == 0 || width > MAX_SIZE || height > MAX_SIZE {
            return Err(SolveError::SizeTooBig);
        }
        Ok(Self {
            width,
            height,
            cells: [0; MAX_INNER],
        })
    }

    pub fn from_rows(rows: &[&[u8]]) -> Result<Self> {
        let height = rows.len();
        let width = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        if height == 0 || width == 0 {
            return Err(SolveError::SizeTooBig);
        }
        if height > MAX_SIZE || width > MAX_SIZE {
            return Err(SolveError::SizeTooBig);
        }
        let mut board = Self::new(width, height)?;
        for (y, row) in rows.iter().enumerate() {
            for (x, byte) in row.iter().enumerate() {
                board.cells[y * MAX_SIZE + x] = tile_from_byte(*byte);
            }
        }
        Ok(board)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn cells(&self) -> &[u8; MAX_INNER] {
        &self.cells
    }

    pub fn cells_mut(&mut self) -> &mut [u8; MAX_INNER] {
        &mut self.cells
    }

    #[inline]
    pub fn flat_index(y: usize, x: usize) -> usize {
        y * MAX_SIZE + x
    }

    #[inline]
    pub fn in_bounds(&self, y: usize, x: usize) -> bool {
        y < self.height && x < self.width
    }

    #[inline]
    pub fn cell(&self, y: usize, x: usize) -> u8 {
        debug_assert!(self.in_bounds(y, x));
        self.cells[y * MAX_SIZE + x]
    }

    #[inline]
    pub fn get(&self, y: isize, x: isize) -> u8 {
        if y < 0 || x < 0 {
            return 0;
        }
        let (y, x) = (y as usize, x as usize);
        if y >= self.height || x >= self.width {
            return 0;
        }
        self.cells[y * MAX_SIZE + x]
    }

    #[inline]
    pub fn get_u(&self, y: usize, x: usize) -> u8 {
        if y >= self.height || x >= self.width {
            return 0;
        }
        self.cells[y * MAX_SIZE + x]
    }

    #[inline]
    pub fn set(&mut self, y: usize, x: usize, value: u8) {
        debug_assert!(self.in_bounds(y, x));
        self.cells[y * MAX_SIZE + x] = value;
    }

    #[inline]
    pub fn or(&mut self, y: usize, x: usize, value: u8) {
        if self.in_bounds(y, x) {
            self.cells[y * MAX_SIZE + x] |= value;
        }
    }

    #[inline]
    pub fn and_not(&mut self, y: usize, x: usize, value: u8) {
        if self.in_bounds(y, x) {
            self.cells[y * MAX_SIZE + x] &= !value;
        }
    }

    /// Neighbor access with C++ sentinel semantics (0 outside the board).
    #[inline]
    pub fn at_dir(&self, y: usize, x: usize, dir: usize) -> u8 {
        self.get(y as isize + DELTA_Y[dir], x as isize + DELTA_X[dir])
    }

    pub fn is_wall(&self, y: isize, x: isize) -> bool {
        self.get(y, x) == WALL
    }

    pub fn is_occupied(&self, y: isize, x: isize) -> bool {
        self.get(y, x) & OCCUPIED != 0
    }

    pub fn zero(&mut self) {
        self.cells = [0; MAX_INNER];
    }

    pub fn copy_board(&self) -> Self {
        *self
    }

    pub fn without_boxes(&self) -> Self {
        let mut board = *self;
        board.clear_boxes_inplace();
        board
    }

    pub fn clear_boxes_inplace(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.cells[y * MAX_SIZE + x] &= !BOX;
            }
        }
    }

    pub fn clear_sokoban_inplace(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.cells[y * MAX_SIZE + x] &= !SOKOBAN;
            }
        }
    }

    pub fn clear_targets_inplace(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.cells[y * MAX_SIZE + x] &= !TARGET;
            }
        }
    }

    pub fn clear_bases_inplace(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.cells[y * MAX_SIZE + x] &= !BASE;
            }
        }
    }

    pub fn clear_deadlock_zone_inplace(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.cells[y * MAX_SIZE + x] &= !DEADLOCK_ZONE;
            }
        }
    }

    pub fn contains_boxes(&self) -> bool {
        (0..self.height).any(|y| (0..self.width).any(|x| self.cell(y, x) & BOX != 0))
    }

    pub fn boxes_in_level(&self) -> usize {
        let mut sum = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & BOX != 0 {
                    sum += 1;
                }
            }
        }
        sum
    }

    pub fn boxes_on_targets(&self) -> usize {
        let mut sum = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                if (self.cell(y, x) & !BASE) == PACKED_BOX {
                    sum += 1;
                }
            }
        }
        sum
    }

    pub fn boxes_on_bases(&self, initial: &Board) -> usize {
        let mut sum = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & BOX == 0 {
                    continue;
                }
                if initial.cell(y, x) & BOX != 0 {
                    sum += 1;
                }
            }
        }
        sum
    }

    pub fn all_boxes_in_place(&self, pull_mode: bool, initial: &Board) -> bool {
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.cell(y, x);
                if cell & BOX == 0 {
                    continue;
                }
                if pull_mode {
                    if initial.cell(y, x) & BOX == 0 {
                        return false;
                    }
                } else if cell & TARGET == 0 {
                    return false;
                }
            }
        }
        true
    }

    pub fn is_solved(
        &self,
        pull_mode: bool,
        initial: &Board,
        initial_sokoban: (usize, usize),
    ) -> bool {
        if pull_mode {
            let (y, x) = initial_sokoban;
            if !self.in_bounds(y, x) || self.cell(y, x) & SOKOBAN == 0 {
                return false;
            }
        }
        self.all_boxes_in_place(pull_mode, initial)
    }

    pub fn all_targets_filled(&self) -> bool {
        for y in 0..self.height {
            for x in 0..self.width {
                if (self.cell(y, x) & !SOKOBAN) == TARGET {
                    return false;
                }
            }
        }
        true
    }

    pub fn sokoban_position(&self) -> Option<(usize, usize)> {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & SOKOBAN != 0 {
                    return Some((y, x));
                }
            }
        }
        None
    }

    pub fn sokoban_cloud_size(&self) -> usize {
        let mut sum = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & SOKOBAN != 0 {
                    sum += 1;
                }
            }
        }
        sum
    }

    pub fn expand_sokoban_cloud(&mut self) {
        let mut queue: Vec<(usize, usize)> = Vec::with_capacity(self.width * self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & SOKOBAN != 0 {
                    queue.push((y, x));
                }
            }
        }
        let mut pos = 0;
        while pos < queue.len() {
            let (y, x) = queue[pos];
            pos += 1;
            for dir in 0..4 {
                let ny = y as isize + DELTA_Y[dir];
                let nx = x as isize + DELTA_X[dir];
                if ny < 0 || nx < 0 {
                    continue;
                }
                let (ny, nx) = (ny as usize, nx as usize);
                if !self.in_bounds(ny, nx) {
                    continue;
                }
                let cell = self.cell(ny, nx);
                if cell & SOKOBAN != 0 || cell & OCCUPIED != 0 {
                    continue;
                }
                self.cells[ny * MAX_SIZE + nx] |= SOKOBAN;
                queue.push((ny, nx));
            }
        }
    }

    pub fn turn_boxes_into_walls(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & BOX != 0 {
                    self.cells[y * MAX_SIZE + x] = WALL;
                }
            }
        }
    }

    pub fn turn_targets_into_walls(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & TARGET != 0 {
                    self.cells[y * MAX_SIZE + x] = WALL;
                }
            }
        }
    }

    pub fn turn_targets_into_packed(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.cell(y, x) & TARGET != 0 {
                    self.cells[y * MAX_SIZE + x] = PACKED_BOX;
                }
            }
        }
    }

    pub fn keep_boxes_in_inner(&mut self, inner: &[bool; MAX_INNER]) {
        for y in 0..self.height {
            for x in 0..self.width {
                if !inner[y * MAX_SIZE + x] && self.cell(y, x) & BOX != 0 {
                    self.cells[y * MAX_SIZE + x] = WALL;
                }
            }
        }
    }

    pub fn and_board(&mut self, mask: &Board) {
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * MAX_SIZE + x;
                self.cells[idx] &= mask.cells[idx];
            }
        }
    }

    pub fn negate(&mut self) -> Result<()> {
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * MAX_SIZE + x;
                if self.cells[idx] > 1 {
                    return Err(SolveError::Internal("negate_board: value > 1".into()));
                }
                self.cells[idx] = 1 - self.cells[idx];
            }
        }
        Ok(())
    }

    pub fn enter_reverse_mode(&mut self, initial: &Board, mark_bases: bool) {
        self.clear_sokoban_inplace();
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * MAX_SIZE + x;
                if self.cells[idx] == BOX {
                    self.cells[idx] = 0;
                }
                if self.cells[idx] == TARGET {
                    self.cells[idx] = BOX | TARGET;
                }
                if mark_bases && initial.cells[idx] & BOX != 0 {
                    self.cells[idx] |= BASE;
                }
            }
        }
    }

    pub fn reverse_tiles(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * MAX_SIZE + x;
                self.cells[idx] ^= 1;
            }
        }
    }

    pub fn put_sokoban_everywhere(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = y * MAX_SIZE + x;
                if self.cells[idx] & OCCUPIED == 0 {
                    self.cells[idx] |= SOKOBAN;
                }
            }
        }
    }

    pub fn mark_indices_with_boxes(&self, coords: &[(u8, u8)], out: &mut [bool; MAX_INNER]) {
        for (index, &(y, x)) in coords.iter().enumerate() {
            out[index] = self.cell(y as usize, x as usize) & BOX != 0;
        }
    }

    pub fn count_walls_around(&self, y: usize, x: usize) -> usize {
        let mut sum = 0;
        for dir in 0..4 {
            if self.at_dir(y, x, dir) == WALL {
                sum += 1;
            }
        }
        sum
    }

    pub fn count_occupied_around(&self, y: usize, x: usize) -> usize {
        let mut sum = 0;
        for dir in 0..4 {
            if self.at_dir(y, x, dir) & OCCUPIED != 0 {
                sum += 1;
            }
        }
        sum
    }

    /// Exact 64-bit hash used by the C++ solver.
    pub fn hash(&self) -> u64 {
        let mut hash: u64 = 1;
        for y in 0..self.height {
            for x in 0..self.width {
                hash = hash
                    .wrapping_mul(12_345)
                    .wrapping_add(self.cell(y, x) as u64)
                    .wrapping_add(hash >> 32);
            }
        }
        hash
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.width * self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                data.push(self.cell(y, x));
            }
        }
        data
    }

    pub fn from_bytes(width: usize, height: usize, data: &[u8]) -> Result<Self> {
        if data.len() != width * height {
            return Err(SolveError::Internal("board byte length mismatch".into()));
        }
        let mut board = Self::new(width, height)?;
        for y in 0..height {
            for x in 0..width {
                board.set(y, x, data[y * width + x]);
            }
        }
        Ok(board)
    }

    /// Serialization used by `solutions.sok` (`save_board_to_file`).
    pub fn to_sok_text(&self) -> String {
        let mut out = String::with_capacity((self.width + 1) * self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                out.push(match self.cell(y, x) {
                    0 => ' ',
                    WALL => '#',
                    BOX => '$',
                    TARGET => '.',
                    v if v == (BOX | TARGET) => '*',
                    SOKOBAN => '@',
                    v if v == (SOKOBAN | TARGET) => '+',
                    _ => ' ',
                });
            }
            out.push('\n');
        }
        out
    }

    /// Plain-text rendering of the full flag set, for diagnostics.
    pub fn display(&self) -> String {
        let mut out = String::with_capacity((self.width + 4) * (self.height + 2));
        out.push_str("  ");
        for x in 0..self.width {
            out.push(char::from_digit((x % 10) as u32, 10).unwrap_or(' '));
        }
        out.push('\n');
        for y in 0..self.height {
            out.push(char::from_digit((y % 10) as u32, 10).unwrap_or(' '));
            out.push(' ');
            for x in 0..self.width {
                let mut cell = self.cell(y, x);
                if cell & DEADLOCK_ZONE != 0 {
                    cell &= !DEADLOCK_ZONE;
                    cell |= BASE;
                }
                out.push(match cell {
                    0 => ' ',
                    WALL => '#',
                    BOX => '$',
                    TARGET => '.',
                    SOKOBAN => '@',
                    v if v == (BOX | TARGET) => '*',
                    v if v == (SOKOBAN | TARGET) => '+',
                    BASE => ' ',
                    v if v == (BASE | SOKOBAN) => '@',
                    v if v == (BASE | BOX) => '$',
                    v if v == (BASE | TARGET | BOX) => '*',
                    v if v == (BASE | TARGET) => '.',
                    v if v == (BASE | TARGET | SOKOBAN) => '+',
                    _ => '?',
                });
            }
            out.push('\n');
        }
        out.push('\n');
        out
    }
}

pub fn tile_from_byte(byte: u8) -> u8 {
    match byte {
        b'_' | b'-' | b' ' => SPACE,
        b'#' => WALL,
        b'$' => BOX,
        b'.' => TARGET,
        b'*' => BOX | TARGET,
        b'@' => SOKOBAN,
        b'+' => SOKOBAN | TARGET,
        _ => SPACE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple() -> Board {
        Board::from_rows(&[b"#####", b"#@$.#", b"#####"]).unwrap()
    }

    #[test]
    fn parses_tiles() {
        let board = simple();
        assert_eq!(board.width(), 5);
        assert_eq!(board.height(), 3);
        assert_eq!(board.cell(1, 1), SOKOBAN);
        assert_eq!(board.cell(1, 2), BOX);
        assert_eq!(board.cell(1, 3), TARGET);
        assert_eq!(board.cell(0, 0), WALL);
    }

    #[test]
    fn outside_is_zero() {
        let board = simple();
        assert_eq!(board.get(-1, 0), 0);
        assert_eq!(board.get(0, -1), 0);
        assert_eq!(board.get(3, 0), 0);
        assert_eq!(board.get(0, 5), 0);
    }

    #[test]
    fn hash_is_stable_and_wrapping() {
        let board = simple();
        let expected = {
            let mut hash: u64 = 1;
            for y in 0..board.height() {
                for x in 0..board.width() {
                    hash = hash
                        .wrapping_mul(12_345)
                        .wrapping_add(board.cell(y, x) as u64)
                        .wrapping_add(hash >> 32);
                }
            }
            hash
        };
        assert_eq!(board.hash(), expected);
        assert_ne!(board.hash(), 0);
    }

    #[test]
    fn byte_round_trip() {
        let board = simple();
        let bytes = board.to_bytes();
        let restored = Board::from_bytes(board.width(), board.height(), &bytes).unwrap();
        assert_eq!(restored, board);
    }

    #[test]
    fn cloud_expands_around_boxes_and_walls() {
        let mut board = Board::from_rows(&[b"#####", b"#@$.#", b"#####"]).unwrap();
        board.expand_sokoban_cloud();
        assert_eq!(board.cell(1, 1), SOKOBAN);
        assert_eq!(board.cell(1, 2), BOX);
        assert_eq!(board.cell(1, 3), TARGET);
    }

    #[test]
    fn sok_text_round_trip_is_stable() {
        let board = simple();
        let text = board.to_sok_text();
        let rows: Vec<&[u8]> = text.lines().map(|line| line.as_bytes()).collect();
        let restored = Board::from_rows(&rows).unwrap();
        assert_eq!(restored, board);
    }
}
