// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Static deadlock patterns from `textfile.cpp`.
//!
//! The C++ solver stores patterns as text rows, parses them into a 10x10 byte
//! array and adds all rotations and the mirrored rotations. This module does
//! the same at first use and keeps the source order (deduplication is
//! order-sensitive).

use std::sync::OnceLock;

use crate::board::Board;
use crate::constants::*;

const NORMAL_DATA: &[&str] = &[
    "PATTERN", "4 3", "#$?", "# $", "# $", "?##", "", "PATTERN", "3 4", "?$$?", "$  #", "$##?", "",
    "PATTERN", "3 4", "?$$?", "$  #", "###?", "", "PATTERN ", "4 3", "##?", "$ #", "$ #", "$$?",
    "", "PATTERN ", "4 4", "?#??", "# $?", "#  $", "?###", "", "PATTERN ", "4 4", "?#??", "# $?",
    "#  $", "?##$", "", "PATTERN ", "3 4", "####", "$  $", "?$#?", "", "PATTERN ", "4 4", "?#$#",
    "#  $", "#  $", "?###", "", "PATTERN", "4 3", "?#?", "$ $", "# #", "$$?", "", "PATTERN ",
    "4 4", "?#??", "# $#", "#  $", "?##?", "", "PATTERN", "4 4", "#$$?", "# $?", "#  #", "#$$?",
    "", "PATTERN", "3 4", "?$##", "#  $", "?$$#", "", "PATTERN", "3 4", "#$$?", "#  #", "#$$?", "",
    "PATTERN", "4 4", "$##?", "$  #", "$ $#", "?$$?", "", "PATTERN ", "4 3", "#$?", "# $", "# $",
    "#$?", "", "PATTERN ", "4 3", "?#?", "# $", "# $", "?#?", "",
];

const PULL_DATA: &[&str] = &[
    "PATTERN", "3 4", "?##?", "#@@#", "?$$?", "", "PATTERN", "4 4", "?#??", "#@$?", "#@@$", "?##?",
    "", "PATTERN", "4 4", "?#??", "#@$?", "#@@$", "?#$?", "", "PATTERN", "4 4", "?##?", "#@@$",
    "#@$?", "?#$?", "", "PATTERN", "4 4", "?##?", "#@@$", "#@@$", "?##?", "", "PATTERN ", "3 4",
    "?$$?", "$@@$", "####", "", "PATTERN", "3 4", "?$#?", "#@@$", "?##?", "", "PATTERN", "3 4",
    "?$$?", "#@@$", "?##?", "", "PATTERN ", "4 4", "?$#?", "#@@$", "#@@#", "?##?",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeadlockPattern {
    pub pat: [[u8; 5]; 5],
    pub y: usize,
    pub x: usize,
    pub box_shift_y: usize,
    pub box_shift_x: usize,
}

pub struct Patterns {
    pub normal: Vec<DeadlockPattern>,
    pub pull: Vec<DeadlockPattern>,
}

static PATTERNS: OnceLock<Patterns> = OnceLock::new();

pub fn patterns() -> &'static Patterns {
    PATTERNS.get_or_init(|| Patterns {
        normal: build_patterns(NORMAL_DATA),
        pull: build_patterns(PULL_DATA),
    })
}

pub fn build_patterns(data: &[&str]) -> Vec<DeadlockPattern> {
    let mut out = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        let line = data[pos];
        pos += 1;
        if line.starts_with("EOF") {
            break;
        }
        if !line.contains("PATTERN") {
            continue;
        }

        let dims = data.get(pos).copied().unwrap_or_default();
        pos += 1;
        let mut parts = dims.split_whitespace();
        let y: usize = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let x: usize = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        assert!(
            (1..=5).contains(&y) && (1..=5).contains(&x),
            "bad pattern dims"
        );

        let mut pattern = DeadlockPattern {
            pat: [[0; 5]; 5],
            y,
            x,
            box_shift_y: 0,
            box_shift_x: 0,
        };

        for row in 0..y {
            let text = data.get(pos).copied().unwrap_or_default();
            pos += 1;
            for col in 0..x {
                let byte = text.as_bytes().get(col).copied().unwrap_or(b' ');
                pattern.pat[row][col] = pattern_tile(byte);
            }
        }

        add_pattern_rotations(&mut pattern, &mut out);
    }

    out
}

fn pattern_tile(byte: u8) -> u8 {
    match byte {
        b'#' => WALL,
        b'$' => BOX,
        b'?' => 0xff,
        b'@' => SOKOBAN,
        b' ' => SPACE,
        b'*' => PACKED_BOX,
        b'+' => SOKOBAN_ON_TARGET,
        _ => 0xff,
    }
}

fn is_same_pattern(a: &DeadlockPattern, b: &DeadlockPattern) -> bool {
    if a.y != b.y || a.x != b.x {
        return false;
    }
    for row in 0..a.y {
        for col in 0..a.x {
            if a.pat[row][col] != b.pat[row][col] {
                return false;
            }
        }
    }
    true
}

fn set_pattern_box_shift(pattern: &mut DeadlockPattern) -> bool {
    for row in (0..pattern.y).rev() {
        for col in (0..pattern.x).rev() {
            if pattern.pat[row][col] == 0xff {
                continue;
            }
            if pattern.pat[row][col] & BOX != 0 {
                pattern.box_shift_y = row;
                pattern.box_shift_x = col;
                return true;
            }
        }
    }
    false
}

fn add_pattern(pattern: &mut DeadlockPattern, out: &mut Vec<DeadlockPattern>) {
    if !set_pattern_box_shift(pattern) {
        return;
    }
    if out
        .iter()
        .any(|existing| is_same_pattern(existing, pattern))
    {
        return;
    }
    out.push(*pattern);
}

fn rotate_clockwise(pattern: &mut DeadlockPattern) {
    let mut rotated = DeadlockPattern {
        pat: [[0; 5]; 5],
        y: pattern.x,
        x: pattern.y,
        box_shift_y: 0,
        box_shift_x: 0,
    };
    for row in 0..pattern.y {
        for col in 0..pattern.x {
            rotated.pat[col][pattern.y - 1 - row] = pattern.pat[row][col];
        }
    }
    *pattern = rotated;
}

fn flip_pattern(pattern: &mut DeadlockPattern) {
    let mut flipped = *pattern;
    for row in 0..pattern.y {
        for col in 0..pattern.x {
            flipped.pat[row][pattern.x - 1 - col] = pattern.pat[row][col];
        }
    }
    *pattern = flipped;
}

fn add_pattern_rotations(pattern: &mut DeadlockPattern, out: &mut Vec<DeadlockPattern>) {
    for _ in 0..4 {
        add_pattern(pattern, out);
        rotate_clockwise(pattern);
    }
    flip_pattern(pattern);
    for _ in 0..4 {
        add_pattern(pattern, out);
        rotate_clockwise(pattern);
    }
}

/// Exact cell matching. `0xff` in the pattern is a wildcard.
pub fn pattern_at(pattern: &DeadlockPattern, board: &Board, y: usize, x: usize) -> bool {
    for row in 0..pattern.y {
        for col in 0..pattern.x {
            let cell = pattern.pat[row][col];
            if cell == 0xff {
                continue;
            }
            if cell != board.get_u(y + row, x + col) {
                return false;
            }
        }
    }
    true
}

/// Port of `check_deadlock_pattern`.
pub fn check_deadlock_pattern(
    pattern: &DeadlockPattern,
    board: &Board,
    box_y: usize,
    box_x: usize,
) -> bool {
    let start_x = (box_x as isize - pattern.x as isize + 1).max(0) as usize;
    let start_y = (box_y as isize - pattern.y as isize + 1).max(0) as usize;
    let end_x = box_x + 1;
    let end_y = box_y + 1;

    for y in start_y..end_y {
        for x in start_x..end_x {
            if board.get_u(y + pattern.box_shift_y, x + pattern.box_shift_x) & BOX == 0 {
                continue;
            }
            if pattern_at(pattern, board, y, x) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    #[test]
    fn loads_and_deduplicates_patterns() {
        let patterns = patterns();
        assert!(!patterns.normal.is_empty());
        assert!(!patterns.pull.is_empty());
        assert!(patterns.normal.len() < 16 * 8);
        assert!(patterns.pull.len() < 9 * 8);
        assert!(patterns.normal.iter().all(|p| p.y >= 1 && p.x >= 1));
    }

    #[test]
    fn matches_a_known_pattern() {
        let board = parse_levels("#######\n#$    #\n# $   #\n# $   #\n##### #\n#     #\n#######\n")
            .remove(0)
            .board
            .unwrap();
        let patterns = patterns();
        let found = patterns
            .normal
            .iter()
            .any(|p| check_deadlock_pattern(p, &board, 3, 2));
        assert!(found);
    }
}
