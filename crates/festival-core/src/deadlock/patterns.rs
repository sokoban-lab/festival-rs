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
    pub(crate) normal_groups: Vec<PatternGroup>,
    pub(crate) pull_groups: Vec<PatternGroup>,
}

#[derive(Clone, Copy)]
struct GroupPattern {
    mask: u128,
    value: u128,
    neighborhood_mask: u64,
    neighborhood_value: u64,
}

/// Patterns with the same dimensions and box anchor share their origin scan.
pub(crate) struct PatternGroup {
    y: usize,
    x: usize,
    shift_y: usize,
    shift_x: usize,
    patterns: Vec<GroupPattern>,
}

static PATTERNS: OnceLock<Patterns> = OnceLock::new();

pub fn patterns() -> &'static Patterns {
    PATTERNS.get_or_init(|| {
        let normal = build_patterns(NORMAL_DATA);
        let pull = build_patterns(PULL_DATA);
        Patterns {
            normal_groups: build_groups(&normal),
            pull_groups: build_groups(&pull),
            normal,
            pull,
        }
    })
}

fn build_groups(patterns: &[DeadlockPattern]) -> Vec<PatternGroup> {
    let mut groups: Vec<PatternGroup> = Vec::new();

    for pattern in patterns {
        let mut mask = 0u128;
        let mut value = 0u128;
        for row in 0..pattern.y {
            for col in 0..pattern.x {
                let cell = pattern.pat[row][col];
                if cell == 0xff {
                    continue;
                }
                let shift = (row * 5 + col) * 5;
                mask |= 0x1f << shift;
                value |= ((cell & 0x1f) as u128) << shift;
            }
        }

        let mut neighborhood_mask = 0u64;
        let mut neighborhood_value = 0u64;
        for dir in 0..8 {
            let row = pattern.box_shift_y as isize + DELTA_Y_8[dir];
            let col = pattern.box_shift_x as isize + DELTA_X_8[dir];
            if row < 0 || col < 0 || row as usize >= pattern.y || col as usize >= pattern.x {
                continue;
            }
            let cell = pattern.pat[row as usize][col as usize];
            if cell == 0xff {
                continue;
            }
            let shift = dir * 5;
            neighborhood_mask |= 0x1f << shift;
            neighborhood_value |= ((cell & 0x1f) as u64) << shift;
        }

        let existing = groups.iter().position(|group| {
            group.y == pattern.y
                && group.x == pattern.x
                && group.shift_y == pattern.box_shift_y
                && group.shift_x == pattern.box_shift_x
        });
        let packed = GroupPattern {
            mask,
            value,
            neighborhood_mask,
            neighborhood_value,
        };
        match existing {
            Some(index) => groups[index].patterns.push(packed),
            None => groups.push(PatternGroup {
                y: pattern.y,
                x: pattern.x,
                shift_y: pattern.box_shift_y,
                shift_x: pattern.box_shift_x,
                patterns: vec![packed],
            }),
        }
    }

    groups
}

/// Packs a `rows`x`cols` window into five bits per cell (all tile values are
/// below 32 once the deadlock-zone bit is cleared).
#[inline]
fn pack_window(board: &Board, y: usize, x: usize, rows: usize, cols: usize) -> u128 {
    let mut window = 0u128;
    for row in 0..rows {
        for col in 0..cols {
            let cell = board.get_u(y + row, x + col);
            debug_assert!(cell < 32);
            window |= (cell as u128) << ((row * 5 + col) * 5);
        }
    }
    window
}

#[inline]
fn neighbor_signature(board: &Board, y: usize, x: usize) -> u64 {
    let mut signature = 0u64;
    for dir in 0..8 {
        let cell = board.get(y as isize + DELTA_Y_8[dir], x as isize + DELTA_X_8[dir]);
        signature |= (cell as u64) << (dir * 5);
    }
    signature
}

pub(crate) fn groups_match(
    groups: &[PatternGroup],
    board: &Board,
    box_y: usize,
    box_x: usize,
) -> bool {
    let mut min_dy = isize::MAX;
    let mut max_dy = isize::MIN;
    let mut min_dx = isize::MAX;
    let mut max_dx = isize::MIN;
    for group in groups {
        min_dy = min_dy.min(group.shift_y as isize - group.y as isize + 1);
        max_dy = max_dy.max(group.shift_y as isize);
        min_dx = min_dx.min(group.shift_x as isize - group.x as isize + 1);
        max_dx = max_dx.max(group.shift_x as isize);
    }

    let start_y = (box_y as isize + min_dy).max(0) as usize;
    let start_x = (box_x as isize + min_dx).max(0) as usize;
    let end_y = ((box_y as isize + max_dy + 1) as usize).min(board.height());
    let end_x = ((box_x as isize + max_dx + 1) as usize).min(board.width());

    for by in start_y..end_y {
        for bx in start_x..end_x {
            if board.cell(by, bx) & BOX == 0 {
                continue;
            }
            let signature = neighbor_signature(board, by, bx);
            for group in groups {
                let origin_y = by as isize - group.shift_y as isize;
                let origin_x = bx as isize - group.shift_x as isize;
                if origin_y < 0 || origin_x < 0 {
                    continue;
                }
                if origin_y > box_y as isize
                    || origin_y < box_y as isize - group.y as isize + 1
                    || origin_x > box_x as isize
                    || origin_x < box_x as isize - group.x as isize + 1
                {
                    continue;
                }
                if !group.patterns.iter().any(|pattern| {
                    signature & pattern.neighborhood_mask == pattern.neighborhood_value
                }) {
                    continue;
                }
                let window = pack_window(
                    board,
                    origin_y as usize,
                    origin_x as usize,
                    group.y,
                    group.x,
                );
                if group.patterns.iter().any(|pattern| {
                    signature & pattern.neighborhood_mask == pattern.neighborhood_value
                        && window & pattern.mask == pattern.value
                }) {
                    return true;
                }
            }
        }
    }
    false
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

    fn reference_match(patterns: &[DeadlockPattern], board: &Board, y: usize, x: usize) -> bool {
        patterns
            .iter()
            .any(|pattern| check_deadlock_pattern(pattern, board, y, x))
    }

    #[test]
    fn grouped_matching_agrees_with_reference() {
        let patterns = patterns();
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let values = [0u8, 1, 2, 4, 6, 8, 12, 16, 24];

        for _ in 0..4000 {
            let width = 1 + (next() % 8) as usize;
            let height = 1 + (next() % 8) as usize;
            let mut board = Board::new(width, height).unwrap();
            for y in 0..height {
                for x in 0..width {
                    let value = values[(next() % values.len() as u64) as usize];
                    board.set(y, x, value);
                }
            }

            for y in 0..height {
                for x in 0..width {
                    assert_eq!(
                        groups_match(&patterns.normal_groups, &board, y, x),
                        reference_match(&patterns.normal, &board, y, x),
                        "normal mismatch at ({y},{x}) on {width}x{height}"
                    );
                    assert_eq!(
                        groups_match(&patterns.pull_groups, &board, y, x),
                        reference_match(&patterns.pull, &board, y, x),
                        "pull mismatch at ({y},{x}) on {width}x{height}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_pattern_is_grouped() {
        let patterns = patterns();
        let grouped: usize = patterns
            .normal_groups
            .iter()
            .map(|group| group.patterns.len())
            .sum();
        assert_eq!(grouped, patterns.normal.len());
        let grouped: usize = patterns
            .pull_groups
            .iter()
            .map(|group| group.patterns.len())
            .sum();
        assert_eq!(grouped, patterns.pull.len());
    }
}
