// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Solution path reconstruction and output (`sol.cpp`).
//!
//! Search nodes store macro pushes. `find_sol_move` replays the player walking
//! path for every push so the final output is a plain LURD string.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::level::Level;
use crate::moves::{Move, apply_move};

#[derive(Clone, Debug, Default)]
pub struct SolutionText {
    pub lurd: String,
    pub moves: usize,
    pub pushes: usize,
    pub solved: bool,
}

/// Port of `find_sol_move`: BFS over `(player_index, box_index)` states,
/// starting from the end position and pulling the box back to the start.
pub fn find_sol_move(
    level: &Level,
    board: &Board,
    start_sok_index: usize,
    start_box_index: usize,
    end_sok_index: usize,
    end_box_index: usize,
) -> Result<Vec<u8>> {
    let n = level.index_num();
    if n == 0 {
        return Err(SolveError::InvalidMove("empty level".into()));
    }
    let space_size = n
        .checked_mul(n)
        .ok_or_else(|| SolveError::Internal("solution space overflow".into()))?;

    let (start_box_y, start_box_x) = level.coord(start_box_index);
    if board.cell(start_box_y, start_box_x) & BOX == 0 {
        return Err(SolveError::Internal("missing box".into()));
    }

    let mut b = *board;
    b.and_not(start_box_y, start_box_x, BOX);

    let mut src = vec![-1i32; space_size];
    let target_index = start_sok_index * n + start_box_index;
    let initial = end_sok_index * n + end_box_index;
    src[initial] = initial as i32;

    let mut current = vec![initial];
    let mut next = Vec::new();

    while !current.is_empty() {
        next.clear();
        for &state in &current {
            let sok_index = state / n;
            let box_index = state % n;
            let (sok_y, sok_x) = level.coord(sok_index);
            let (box_y, box_x) = level.coord(box_index);

            for dir in 0..4 {
                let ns_y = sok_y as isize + DELTA_Y[dir];
                let ns_x = sok_x as isize + DELTA_X[dir];
                if ns_y == box_y as isize && ns_x == box_x as isize {
                    continue;
                }
                if b.get(ns_y, ns_x) & OCCUPIED != 0 {
                    continue;
                }
                if ns_y < 0 || ns_x < 0 {
                    continue;
                }
                let (ns_y, ns_x) = (ns_y as usize, ns_x as usize);
                let Some(ns_index) = level.index_at(ns_y, ns_x) else {
                    continue;
                };
                let next_state = ns_index * n + box_index;
                if src[next_state] != -1 {
                    continue;
                }
                src[next_state] = state as i32;
                next.push(next_state);
            }

            for dir in 0..4 {
                let nb_y = box_y as isize + DELTA_Y[dir];
                let nb_x = box_x as isize + DELTA_X[dir];
                if nb_y != sok_y as isize || nb_x != sok_x as isize {
                    continue;
                }
                let ns_y = sok_y as isize + DELTA_Y[dir];
                let ns_x = sok_x as isize + DELTA_X[dir];
                if b.get(ns_y, ns_x) & OCCUPIED != 0 {
                    continue;
                }
                if nb_y < 0 || nb_x < 0 || ns_y < 0 || ns_x < 0 {
                    continue;
                }
                let (nb_y, nb_x) = (nb_y as usize, nb_x as usize);
                let (ns_y, ns_x) = (ns_y as usize, ns_x as usize);
                let Some(nb_index) = level.index_at(nb_y, nb_x) else {
                    continue;
                };
                let Some(ns_index) = level.index_at(ns_y, ns_x) else {
                    continue;
                };
                let next_state = ns_index * n + nb_index;
                if src[next_state] != -1 {
                    continue;
                }
                src[next_state] = state as i32;
                next.push(next_state);
            }
        }

        if src[target_index] != -1 {
            break;
        }
        std::mem::swap(&mut current, &mut next);
    }

    if src[target_index] == -1 {
        return Err(SolveError::Internal("could not find player path".into()));
    }

    let mut result = Vec::new();
    let mut cursor = target_index;
    loop {
        let previous = src[cursor];
        if previous < 0 {
            return Err(SolveError::Internal("broken solution path".into()));
        }
        let previous = previous as usize;
        if previous == cursor {
            break;
        }

        let sok_index = cursor / n;
        let next_sok_index = previous / n;
        let box_index = cursor % n;
        let next_box_index = previous % n;
        let (sok_y, sok_x) = level.coord(sok_index);
        let (next_sok_y, next_sok_x) = level.coord(next_sok_index);

        let mut letter = b' ';
        if next_sok_y as isize == sok_y as isize + 1 {
            letter = b'd';
        }
        if next_sok_y as isize == sok_y as isize - 1 {
            letter = b'u';
        }
        if next_sok_x as isize == sok_x as isize + 1 {
            letter = b'r';
        }
        if next_sok_x as isize == sok_x as isize - 1 {
            letter = b'l';
        }
        if letter == b' ' {
            return Err(SolveError::Internal("bad player path step".into()));
        }
        if box_index != next_box_index {
            letter = letter.to_ascii_uppercase();
        }

        result.push(letter);
        cursor = previous;
    }

    Ok(result)
}

/// Converts macro pushes into a LURD string and counts moves/pushes.
pub fn build_lurd(level: &Level, moves: &[Move]) -> Result<SolutionText> {
    let mut board = level.initial;
    let (mut sok_y, mut sok_x) = level.initial_sokoban;
    let mut sok_index = level
        .index_at(sok_y, sok_x)
        .ok_or(SolveError::InvalidBoard)?;

    let mut lurd = Vec::new();
    let mut pushes = 0usize;

    for mv in moves {
        let (end_y, end_x) = level.coord(mv.to as usize);
        let dir = mv.sokoban_position as usize;
        let next_sok_y = end_y as isize + DELTA_Y[dir];
        let next_sok_x = end_x as isize + DELTA_X[dir];
        if next_sok_y < 0 || next_sok_x < 0 {
            return Err(SolveError::Internal("push leaves the board".into()));
        }
        let (next_sok_y, next_sok_x) = (next_sok_y as usize, next_sok_x as usize);
        let next_sok_index = level
            .index_at(next_sok_y, next_sok_x)
            .ok_or_else(|| SolveError::Internal("push leaves the inner area".into()))?;

        let path = find_sol_move(
            level,
            &board,
            sok_index,
            mv.from as usize,
            next_sok_index,
            mv.to as usize,
        )?;
        for &byte in &path {
            if byte.is_ascii_uppercase() {
                pushes += 1;
            }
        }
        lurd.extend_from_slice(&path);

        sok_y = next_sok_y;
        sok_x = next_sok_x;
        sok_index = next_sok_index;

        apply_move(&mut board, mv, SearchMode::Normal, level);
    }

    let solved = board.all_boxes_in_place(false, &level.initial);
    let _ = (sok_y, sok_x);
    let lurd =
        String::from_utf8(lurd).map_err(|_| SolveError::Internal("non-ascii solution".into()))?;

    Ok(SolutionText {
        moves: lurd.len(),
        pushes,
        solved,
        lurd,
    })
}

pub fn write_solutions_header(
    path: &Path,
    level_set_name: &str,
    solver_name: &str,
) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "{solver_name} solution file")?;
    writeln!(file, "===========================")?;
    writeln!(file, "Collection: {level_set_name}")?;
    Ok(())
}

pub fn append_level_to_solutions(
    path: &Path,
    level_number: usize,
    title: &str,
    initial: &Board,
) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    writeln!(file)?;
    if title == "None" {
        writeln!(file, "Level {level_number}")?;
    } else {
        writeln!(file, "{title}")?;
    }
    writeln!(file)?;
    write!(file, "{}", initial.to_sok_text())?;
    writeln!(file)?;
    Ok(())
}

pub fn append_solution(path: &Path, lurd: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    writeln!(file, "Solution")?;
    if !lurd.is_empty() {
        writeln!(file, "{lurd}")?;
    }
    Ok(())
}

pub fn append_solution_time(path: &Path, elapsed: Duration, solver_name: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    let (date, time) = utc_now_strings();
    writeln!(file)?;
    writeln!(file, "Solver: {solver_name}")?;
    writeln!(file, "Solver time: {}", format_hms(elapsed.as_secs()))?;
    writeln!(file, "Solver date: {date}  {time}")?;
    writeln!(file)?;
    Ok(())
}

pub fn write_times_header(path: &Path, solver_name: &str) -> io::Result<()> {
    let mut file = File::create(path)?;
    let (date, time) = utc_now_strings();
    writeln!(
        file,
        "--------------------------------------------------------------------------------------"
    )?;
    writeln!(file, "Sokoban Solver Statistics - {solver_name}")?;
    writeln!(file, "Date: {date}  {time}")?;
    writeln!(
        file,
        "--------------------------------------------------------------------------------------"
    )?;
    writeln!(
        file,
        "   Task       Time Plugin               Result                     Moves  Pushes Level"
    )?;
    writeln!(
        file,
        "--------------------------------------------------------------------------------------"
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn append_times_row(
    path: &Path,
    record: usize,
    elapsed: Duration,
    solver_name: &str,
    solved: bool,
    failure_reason: &str,
    moves: usize,
    pushes: usize,
    title: &str,
) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    write!(file, "{record:7}  ")?;
    write!(file, " {} ", format_hms(elapsed.as_secs()))?;
    write!(file, "{solver_name:<21}")?;
    if solved {
        write!(file, "OK                     ")?;
        write!(file, "{moves:9}{pushes:8} ")?;
    } else {
        write!(file, "{failure_reason:<23}")?;
        write!(file, "                  ")?;
    }
    writeln!(file, "{title}")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn write_times_footer(
    path: &Path,
    total_levels: usize,
    solved_levels: usize,
    total_moves: usize,
    total_pushes: usize,
    total_elapsed: Duration,
) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    let failed = total_levels - solved_levels;
    writeln!(
        file,
        "--------------------------------------------------------------------------------------"
    )?;
    write!(file, ":{total_levels:6}  ")?;
    write!(file, " {} ", format_hms(total_elapsed.as_secs()))?;
    let summary = format!("Total                OK: {solved_levels} Failed: {failed}      ");
    write!(file, "{summary}")?;
    for _ in summary.len()..44 {
        write!(file, " ")?;
    }
    writeln!(file, "{total_moves:9}{total_pushes:8}     :")?;
    writeln!(
        file,
        "--------------------------------------------------------------------------------------"
    )?;
    Ok(())
}

pub fn format_hms(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
}

/// UTC date/time without an external dependency. The C++ solver prints local
/// time; only the timestamp text differs.
fn utc_now_strings() -> (String, String) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0) as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let time = format!(
        "{:02}:{:02}:{:02}",
        day_seconds / 3600,
        (day_seconds / 60) % 60,
        day_seconds % 60
    );
    (format!("{year:04}-{month:02}-{day:02}"), time)
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::lurd;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn finds_the_player_path_for_a_push() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let mut helper = crate::search::helper::Helper::default();
        let clock = crate::solver::SearchClock::new(Some(Duration::from_secs(60)), None);
        let ctx = crate::solver::LevelContext::analyse(level.clone(), &clock).unwrap();
        let (moves, _) = crate::moves::find_possible_moves(
            &ctx,
            &level.board,
            false,
            SearchMode::Normal,
            &mut helper,
        )
        .unwrap();
        let target = level.index_at(1, 5).unwrap() as u16;
        let solving: Vec<Move> = moves.into_iter().filter(|mv| mv.to == target).collect();
        let text = build_lurd(&level, &solving).unwrap();
        assert!(text.solved);
        assert_eq!(text.pushes, 2);
        let stats = lurd::replay(&level.initial, &text.lurd).unwrap();
        assert!(stats.solved);
    }

    #[test]
    fn hms_format_matches_cpp() {
        assert_eq!(format_hms(0), "00:00:00");
        assert_eq!(format_hms(3661), "01:01:01");
    }
}
