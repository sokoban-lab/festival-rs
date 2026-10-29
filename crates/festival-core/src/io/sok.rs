//! `.sok` level parser.
//!
//! This mirrors the line-oriented state machine in `io.cpp`. In particular:
//! - a level line must be at least 3 characters long and contain only legal
//!   tiles, and lines without `#` are accepted only as decorative empty rows;
//! - a `Title:` line, a `;` subtitle line, or any other non-level line closes
//!   the current level;
//! - rows longer than `MAX_SIZE - 1` are truncated and mark the level invalid.

use std::fs;
use std::path::{Path, PathBuf};

use crate::board::Board;
use crate::constants::MAX_SIZE;
use crate::error::{Result, SolveError};

const LEGAL: &[u8] = b"_- #$.*@+";

#[derive(Clone, Debug)]
pub struct RawLevel {
    pub board: Option<Board>,
    pub title: String,
    pub size_error: bool,
}

pub fn parse_levels(text: &str) -> Vec<RawLevel> {
    let mut levels = Vec::new();
    let mut current: Option<Vec<Vec<u8>>> = None;
    let mut ok = true;
    let mut tmp_title = String::from("None");

    for line in text.lines() {
        let bytes = line.as_bytes();
        if bytes.contains(&b'\'') {
            tmp_title = line.to_string();
        }

        if is_sokoban_line(bytes) {
            if current.is_none() {
                current = Some(Vec::new());
                ok = true;
            }
            let rows = current.as_mut().expect("just inserted");

            if bytes.len() >= MAX_SIZE {
                ok = false;
                rows.push(bytes[..MAX_SIZE - 1].to_vec());
            } else {
                rows.push(bytes.to_vec());
            }

            if rows.len() >= MAX_SIZE {
                ok = false;
                rows.pop();
            }
        } else if let Some(rows) = current.take() {
            let title = if let Some(rest) = line.strip_prefix(';') {
                rest.trim_start_matches(' ').to_string()
            } else if let Some(rest) = title_value(line) {
                rest.to_string()
            } else {
                tmp_title.clone()
            };
            levels.push(finish_level(rows, title, ok));
            tmp_title = String::from("None");
        } else if let Some(rest) = title_value(line) {
            // The C++ code applies a title line to the level that was just
            // closed (current_level_num has already been incremented).
            if let Some(last) = levels.last_mut() {
                last.title = rest.to_string();
            }
        }
    }

    if let Some(rows) = current.take() {
        levels.push(finish_level(rows, tmp_title, ok));
    }

    levels
}

fn finish_level(rows: Vec<Vec<u8>>, title: String, ok: bool) -> RawLevel {
    let board = if rows.is_empty() {
        None
    } else {
        let slices: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
        Board::from_rows(&slices).ok()
    };
    RawLevel {
        board,
        title,
        size_error: !ok || board.is_none(),
    }
}

/// Reproduces `strncmp(s, "Title:", 6)` / `strncmp(s, "Title : ", 7)` followed
/// by `strcpy(..., s + 7)` / `strcpy(..., s + 8)`.
fn title_value(line: &str) -> Option<&str> {
    for prefix in ["Title:", "Title :"] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(rest.get(1..).unwrap_or(""));
        }
    }
    None
}

fn is_sokoban_line(s: &[u8]) -> bool {
    if s.len() < 3 {
        return false;
    }
    if !s.contains(&b'#') && !is_empty_row(s) {
        return false;
    }
    has_only_legal_characters(s)
}

fn is_empty_row(s: &[u8]) -> bool {
    if s.len() < 5 {
        return false;
    }
    if !has_only_legal_characters(s) {
        return false;
    }
    !s.iter().any(|b| *b == b' ' || *b == b'\t')
}

fn has_only_legal_characters(s: &[u8]) -> bool {
    s.iter().all(|b| LEGAL.contains(b))
}

pub fn resolve_level_file(dir: &Path, name: &str) -> PathBuf {
    if name.contains('.') || name.contains('\\') || name.contains('/') {
        PathBuf::from(name)
    } else {
        dir.join("levels").join(format!("{name}.sok"))
    }
}

pub fn load_levels(path: &Path) -> Result<Vec<RawLevel>> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let levels = parse_levels(&text);
    if levels.is_empty() {
        return Err(SolveError::LevelNotFound);
    }
    Ok(levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{BOX, TARGET, WALL};

    #[test]
    fn parses_two_levels_and_titles() {
        let text = "\
Level 1
#####
#@$.#
#####
Title: First
#####
#@ $#
#  .#
#####
; Second
";
        let levels = parse_levels(text);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].title, "First");
        assert_eq!(levels[1].title, "Second");
        assert!(!levels[0].size_error);
    }

    #[test]
    fn underscores_and_dashes_are_floor() {
        let levels = parse_levels("#####\n#@_-.#\n#####\n");
        assert_eq!(levels.len(), 1);
        let board = levels[0].board.unwrap();
        assert_eq!(board.cell(1, 2), 0);
        assert_eq!(board.cell(1, 3), 0);
        assert_eq!(board.cell(1, 4), TARGET);
    }

    #[test]
    fn decorative_empty_rows_do_not_split_level() {
        let text = "#####\n#@$.#\n#####\n-----\n#####\n#  .#\n#####\n";
        let levels = parse_levels(text);
        assert_eq!(levels.len(), 1);
        let board = levels[0].board.unwrap();
        assert_eq!(board.height(), 7);
    }

    #[test]
    fn too_wide_row_marks_size_error() {
        let mut line = String::from("#");
        line.push_str(&" ".repeat(60));
        line.push('#');
        let text = format!("{line}\n#@#\n");
        let levels = parse_levels(&text);
        assert_eq!(levels.len(), 1);
        assert!(levels[0].size_error);
    }

    #[test]
    fn parses_flags() {
        let levels = parse_levels("#####\n#+*.#\n#####\n");
        let board = levels[0].board.unwrap();
        assert_eq!(
            board.cell(1, 1) & crate::constants::SOKOBAN,
            crate::constants::SOKOBAN
        );
        assert_eq!(board.cell(1, 1) & TARGET, TARGET);
        assert_eq!(board.cell(1, 2) & BOX, BOX);
        assert_eq!(board.cell(1, 4), WALL);
    }

    #[test]
    fn title_without_level_is_ignored() {
        let levels = parse_levels("Title: Nope\n#####\n#@$.#\n#####\n");
        assert_eq!(levels[0].title, "None");
    }
}
