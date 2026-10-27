// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! End-to-end driver for static analysis and differential baselines.
//!
//! Usage:
//!   `festival-bench [levels.sok]` — static analysis CSV
//!   `festival-bench verify-solutions <levels.sok> <solutions.sok>`

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use festival_core::Board;
use festival_core::io::lurd;
use festival_core::io::sok::{load_levels, resolve_level_file};
use festival_core::moves::{Move, MoveAttr, find_possible_moves};
use festival_core::search::helper::Helper;
use festival_core::{Level, LevelContext, SearchClock};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

    if args.first().map(String::as_str) == Some("verify-solutions") {
        if args.len() != 3 {
            eprintln!("usage: festival-bench verify-solutions <levels.sok> <solutions.sok>");
            return ExitCode::FAILURE;
        }
        return verify_solutions(Path::new(&args[1]), Path::new(&args[2]));
    }

    if args.first().map(String::as_str) == Some("deadlock-diff") {
        if args.len() < 5 {
            eprintln!(
                "usage: festival-bench deadlock-diff <levels.sok> <dir> <from> <to> [--cache-off]"
            );
            return ExitCode::FAILURE;
        }
        let from: usize = args[3].parse().unwrap_or(1);
        let to: usize = args[4].parse().unwrap_or(from);
        let cache_off = args.iter().any(|arg| arg == "--cache-off");
        return deadlock_diff(
            Path::new(&args[1]),
            Path::new(&args[2]),
            from,
            to,
            cache_off,
        );
    }

    if args.first().map(String::as_str) == Some("deadlock-explain") {
        if args.len() < 4 {
            eprintln!(
                "usage: festival-bench deadlock-explain <levels.sok> <level> <hex> [--cache-off]"
            );
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(1);
        let cache_off = args.iter().any(|arg| arg == "--cache-off");
        return deadlock_explain(Path::new(&args[1]), level_number, &args[3], cache_off);
    }

    analyse(args.first().map(String::as_str).unwrap_or("XSokoban"))
}

fn deadlock_explain(
    levels_path: &Path,
    level_number: usize,
    hex: &str,
    cache_off: bool,
) -> ExitCode {
    let raws = load_levels(levels_path).expect("levels");
    let raw = raws
        .into_iter()
        .nth(level_number.saturating_sub(1))
        .expect("level");
    let level = Level::preprocess(raw).expect("preprocess");
    let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
    let ctx = LevelContext::analyse(level, &clock).expect("analysis");
    let mut state = festival_core::deadlock::state::DeadlockState::prepare(&ctx).expect("state");
    state.cache_enabled = !cache_off;

    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let data = hex_board(hex, width, height).expect("hex");
    let board = Board::from_bytes(width, height, &data).expect("board");
    println!("{}", board.display());
    println!("inner:");
    for y in 0..height {
        print!("  ");
        for x in 0..width {
            print!("{}", if ctx.level.is_inner(y, x) { '1' } else { '.' });
        }
        println!();
    }
    let mut pull_board = board;
    pull_board.clear_bases_inplace();
    festival_core::deadlock::zone::set_deadlock_zone_in_pull_mode(&ctx.level, &mut pull_board);
    println!(
        "pull zone ({:?} cells):",
        festival_core::deadlock::zone::size_of_deadlock_zone(&pull_board)
    );
    println!("{}", pull_board.display());
    let mut cleared = pull_board;
    festival_core::deadlock::zone::clear_boxes_out_of_deadlock_zone(&mut cleared);
    println!(
        "cleared boxes ({}/{}):",
        cleared.boxes_in_level(),
        ctx.level.box_count
    );
    println!("{}", cleared.display());
    for pull_mode in [false, true] {
        println!("pull_mode={pull_mode}");
        for (name, verdict) in festival_core::deadlock::oracle::explain_board(
            &ctx,
            &mut state,
            &board,
            pull_mode,
            festival_core::constants::SearchMode::Normal,
        ) {
            println!("  {name}: {verdict}");
        }
    }
    ExitCode::SUCCESS
}

fn analyse(name: &str) -> ExitCode {
    let path = resolve_level_file(&PathBuf::from("."), name);
    let raws = match load_levels(&path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    };

    println!("level,title,status,boxes,inner,rooms,hotspots,ms,push_pairs");

    for (index, raw) in raws.into_iter().enumerate() {
        let title = raw.title.clone().replace(',', " ");
        let started = Instant::now();
        let clock = SearchClock::new(Some(Duration::from_secs(600)), None);

        let level = match Level::preprocess(raw) {
            Ok(level) => level,
            Err(err) => {
                println!(
                    "{},{},fail:{},0,0,0,0,{:.3},0",
                    index + 1,
                    title,
                    err.reason(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
                continue;
            }
        };

        match LevelContext::analyse(level, &clock) {
            Ok(context) => {
                let level = &context.level;
                let push_pairs = level.index_num() * level.index_num();
                println!(
                    "{},{},ok,{},{},{},{},{:.3},{}",
                    index + 1,
                    title,
                    level.box_count,
                    level.index_num(),
                    context.rooms.num,
                    context.hotspots.score(level, &level.board),
                    started.elapsed().as_secs_f64() * 1000.0,
                    push_pairs
                );
            }
            Err(err) => {
                println!(
                    "{},{},fail:{},0,0,0,0,{:.3},0",
                    index + 1,
                    title,
                    err.reason(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
        }
    }

    ExitCode::SUCCESS
}

fn raw_candidates(ctx: &LevelContext, board: &Board) -> (Vec<Move>, Vec<Move>) {
    let level = &ctx.level;
    let mut pushes = Vec::new();
    let mut pulls = Vec::new();

    for index in 0..level.index_num() {
        let (y, x) = level.coord(index);
        if board.cell(y, x) & festival_core::constants::BOX == 0 {
            continue;
        }
        for dir in 0..4 {
            let dy = festival_core::constants::DELTA_Y[dir];
            let dx = festival_core::constants::DELTA_X[dir];
            let to_y = y as isize + dy;
            let to_x = x as isize + dx;
            if to_y < 0
                || to_x < 0
                || to_y >= board.height() as isize
                || to_x >= board.width() as isize
            {
                continue;
            }
            let (to_y, to_x) = (to_y as usize, to_x as usize);

            if board.cell(to_y, to_x) & festival_core::constants::OCCUPIED == 0 {
                let pusher_y = y as isize - dy;
                let pusher_x = x as isize - dx;
                if board.get(pusher_y, pusher_x) & festival_core::constants::SOKOBAN != 0 {
                    if let Some(to) = level.index_at(to_y, to_x) {
                        pushes.push(Move {
                            from: index as u16,
                            to: to as u16,
                            sokoban_position: ((dir + 2) & 3) as u8,
                            kill: false,
                            base: false,
                            pull: false,
                            attr: MoveAttr::default(),
                        });
                    }
                }
            }

            let pull_to_y = y as isize + dy * 2;
            let pull_to_x = x as isize + dx * 2;
            if pull_to_y < 0
                || pull_to_x < 0
                || pull_to_y >= board.height() as isize
                || pull_to_x >= board.width() as isize
            {
                continue;
            }
            let (pull_to_y, pull_to_x) = (pull_to_y as usize, pull_to_x as usize);
            if board.cell(to_y, to_x) & festival_core::constants::SOKOBAN == 0 {
                continue;
            }
            if board.cell(pull_to_y, pull_to_x) & festival_core::constants::OCCUPIED != 0 {
                continue;
            }
            let Some(to) = level.index_at(pull_to_y, pull_to_x) else {
                continue;
            };
            pulls.push(Move {
                from: index as u16,
                to: to as u16,
                sokoban_position: ((dir + 2) & 3) as u8,
                kill: false,
                base: false,
                pull: true,
                attr: MoveAttr::default(),
            });
        }
    }
    (pushes, pulls)
}

fn hex_board(text: &str, width: usize, height: usize) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() != width * height * 2 {
        return None;
    }
    let mut data = Vec::with_capacity(width * height);
    for i in 0..(width * height) {
        let hi = (bytes[i * 2] as char).to_digit(16)?;
        let lo = (bytes[i * 2 + 1] as char).to_digit(16)?;
        data.push(((hi << 4) | lo) as u8);
    }
    Some(data)
}

#[allow(clippy::too_many_arguments)]
fn deadlock_diff(
    levels_path: &Path,
    dir: &Path,
    from: usize,
    to: usize,
    cache_off: bool,
) -> ExitCode {
    use festival_core::deadlock::oracle::{board_is_deadlocked, move_is_deadlocked};
    use festival_core::deadlock::state::DeadlockState;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut total_boards = 0usize;
    let mut total_moves = 0usize;
    let mut board_mismatches = 0usize;
    let mut move_mismatches = 0usize;
    let mut false_positives = 0usize;
    let mut false_negatives = 0usize;
    let mut cache_diffs = 0usize;
    let mut failures = 0usize;
    let mut eval_time = Duration::ZERO;
    let mut move_time = Duration::ZERO;
    let mut cpp_board_calls = 0u64;
    let mut cpp_board_ns = 0u64;
    let mut cpp_move_calls = 0u64;
    let mut cpp_move_ns = 0u64;

    println!("level,boards,moves,board_diff,move_diff,cache_diff,eval_s");

    for (index, raw) in raws.into_iter().enumerate() {
        let level_number = index + 1;
        if level_number < from || level_number > to {
            continue;
        }

        let level = match Level::preprocess(raw) {
            Ok(level) => level,
            Err(err) => {
                eprintln!("level {level_number}: preprocess failed: {}", err.reason());
                failures += 1;
                continue;
            }
        };
        let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
        let ctx = match LevelContext::analyse(level, &clock) {
            Ok(ctx) => ctx,
            Err(err) => {
                eprintln!("level {level_number}: analysis failed: {err}");
                failures += 1;
                continue;
            }
        };
        let mut state = match DeadlockState::prepare(&ctx) {
            Ok(state) => state,
            Err(err) => {
                eprintln!("level {level_number}: deadlock prep failed: {err}");
                failures += 1;
                continue;
            }
        };
        let mut no_cache_state = if cache_off {
            let mut state = DeadlockState::prepare(&ctx).unwrap_or_default();
            state.cache_enabled = false;
            Some(state)
        } else {
            None
        };

        let width = ctx.level.initial.width();
        let height = ctx.level.initial.height();
        let boards_path = dir.join(format!("cpp-{level_number}.boards"));
        let moves_path = dir.join(format!("cpp-{level_number}.moves"));
        let info_path = dir.join(format!("cpp-{level_number}.info"));
        if let Ok(info) = fs::read_to_string(&info_path) {
            for line in info.lines() {
                let mut parts = line.split_whitespace();
                let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
                    continue;
                };
                let value: u64 = value.parse().unwrap_or(0);
                match key {
                    "board_calls" => cpp_board_calls += value,
                    "board_ns" => cpp_board_ns += value,
                    "move_calls" => cpp_move_calls += value,
                    "move_ns" => cpp_move_ns += value,
                    _ => {}
                }
            }
        }
        let boards_text = match fs::read_to_string(&boards_path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("{}: {err}", boards_path.display());
                failures += 1;
                continue;
            }
        };
        let moves_text = fs::read_to_string(&moves_path).unwrap_or_default();
        let move_lines: Vec<&str> = moves_text
            .lines()
            .filter(|line| line.split_whitespace().count() >= 2)
            .collect();

        let mut level_boards = 0usize;
        let mut level_moves = 0usize;
        let mut level_board_diff = 0usize;
        let mut level_move_diff = 0usize;
        let mut level_cache_diff = 0usize;
        let mut level_eval = Duration::ZERO;
        let mut helper = Helper::default();
        let mut helper_off = Helper::default();

        for line in boards_text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() != 4 {
                continue;
            }
            let cpp_hash: u64 = fields[0].parse().unwrap_or(0);
            let Some(data) = hex_board(fields[1], width, height) else {
                eprintln!("level {level_number}: bad board hex");
                failures += 1;
                continue;
            };
            let board = Board::from_bytes(width, height, &data).expect("board bytes");
            if board.hash() != cpp_hash {
                eprintln!(
                    "level {level_number}: hash mismatch with C++ ({:016x} vs {:016x})",
                    board.hash(),
                    cpp_hash
                );
                failures += 1;
            }
            level_boards += 1;
            total_boards += 1;

            let cpp_push = fields[2] != "0";
            let cpp_pull = fields[3] != "0";

            let started = Instant::now();
            let rust_push = board_is_deadlocked(
                &ctx,
                &mut state,
                &board,
                false,
                festival_core::constants::SearchMode::Normal,
            )
            .unwrap_or(false);
            let rust_pull = board_is_deadlocked(
                &ctx,
                &mut state,
                &board,
                true,
                festival_core::constants::SearchMode::Normal,
            )
            .unwrap_or(false);
            let elapsed = started.elapsed();
            eval_time += elapsed;
            level_eval += elapsed;

            let push_diff = rust_push != cpp_push;
            let pull_diff = rust_pull != cpp_pull;
            if push_diff || pull_diff {
                level_board_diff += 1;
                board_mismatches += 1;
                if rust_push && !cpp_push {
                    false_positives += 1;
                }
                if !rust_push && cpp_push {
                    false_negatives += 1;
                }
                if rust_pull && !cpp_pull {
                    false_positives += 1;
                }
                if !rust_pull && cpp_pull {
                    false_negatives += 1;
                }
                let report_cap = if std::env::var("FESTIVAL_DIFF_VERBOSE").is_ok() {
                    usize::MAX
                } else {
                    3
                };
                if level_board_diff <= report_cap {
                    eprintln!(
                        "level {level_number}: board deadlock mismatch (cpp push/pull {cpp_push}/{cpp_pull}, rust {rust_push}/{rust_pull})"
                    );
                    eprintln!("{}", board.display());
                    let hex: String = data.iter().map(|byte| format!("{byte:02x}")).collect();
                    eprintln!("hex {hex}");
                }
            }

            if let Some(other) = no_cache_state.as_mut() {
                let other_push = board_is_deadlocked(
                    &ctx,
                    other,
                    &board,
                    false,
                    festival_core::constants::SearchMode::Normal,
                )
                .unwrap_or(false);
                let other_pull = board_is_deadlocked(
                    &ctx,
                    other,
                    &board,
                    true,
                    festival_core::constants::SearchMode::Normal,
                )
                .unwrap_or(false);
                if other_push != rust_push || other_pull != rust_pull {
                    level_cache_diff += 1;
                    cache_diffs += 1;
                    if level_cache_diff <= 3 {
                        let hex: String = data.iter().map(|byte| format!("{byte:02x}")).collect();
                        eprintln!(
                            "level {level_number}: cache-off verdict mismatch (cache push/pull {rust_push}/{rust_pull}, off {other_push}/{other_pull}) hex {hex}"
                        );
                    }
                }
            }

            let sample = level_boards - 1;
            if let Some(line) = move_lines.get(sample) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.first().and_then(|v| v.parse::<usize>().ok()) == Some(sample) {
                    let (pushes, pulls) = raw_candidates(&ctx, &board);
                    let mut at = 1usize;
                    let npush: usize = fields[at].parse().unwrap_or(0);
                    at += 1;
                    if npush != pushes.len() {
                        eprintln!(
                            "level {level_number} sample {sample}: push candidate count {} vs {}",
                            npush,
                            pushes.len()
                        );
                        failures += 1;
                    } else {
                        let started = Instant::now();
                        for mv in &pushes {
                            let cpp = fields[at] != "0";
                            at += 1;
                            let rust = move_is_deadlocked(
                                &ctx,
                                &mut state,
                                &board,
                                mv,
                                false,
                                festival_core::constants::SearchMode::Normal,
                            )
                            .unwrap_or(false);
                            total_moves += 1;
                            level_moves += 1;
                            if rust != cpp {
                                level_move_diff += 1;
                                move_mismatches += 1;
                                if rust {
                                    false_positives += 1;
                                } else {
                                    false_negatives += 1;
                                }
                                if level_move_diff <= 3 {
                                    eprintln!(
                                        "level {level_number} sample {sample}: push move mismatch (cpp {cpp}, rust {rust}) {mv:?}"
                                    );
                                }
                            }
                        }

                        let npull: usize = fields.get(at).and_then(|v| v.parse().ok()).unwrap_or(0);
                        at += 1;
                        if npull != pulls.len() {
                            eprintln!(
                                "level {level_number} sample {sample}: pull candidate count {} vs {}",
                                npull,
                                pulls.len()
                            );
                            failures += 1;
                        } else {
                            for (i, mv) in pulls.iter().enumerate() {
                                let cpp = fields.get(at + i).map(|v| *v != "0").unwrap_or(false);
                                let rust = move_is_deadlocked(
                                    &ctx,
                                    &mut state,
                                    &board,
                                    mv,
                                    true,
                                    festival_core::constants::SearchMode::Normal,
                                )
                                .unwrap_or(false);
                                total_moves += 1;
                                level_moves += 1;
                                if rust != cpp {
                                    level_move_diff += 1;
                                    move_mismatches += 1;
                                    if rust {
                                        false_positives += 1;
                                    } else {
                                        false_negatives += 1;
                                    }
                                    if level_move_diff <= 3 {
                                        eprintln!(
                                            "level {level_number} sample {sample}: pull move mismatch (cpp {cpp}, rust {rust}) {mv:?}"
                                        );
                                    }
                                }
                            }
                        }
                        move_time += started.elapsed();

                        if let Some(other) = no_cache_state.as_mut() {
                            for mv in &pushes {
                                let _ = move_is_deadlocked(
                                    &ctx,
                                    other,
                                    &board,
                                    mv,
                                    false,
                                    festival_core::constants::SearchMode::Normal,
                                );
                            }
                            for mv in &pulls {
                                let _ = move_is_deadlocked(
                                    &ctx,
                                    other,
                                    &board,
                                    mv,
                                    true,
                                    festival_core::constants::SearchMode::Normal,
                                );
                            }
                        }
                    }
                }
            }

            // C++'s generator calls the solver's pruned move generator for each
            // sampled board; reproduce that call so both caches evolve alike.
            let _ = find_possible_moves(
                &ctx,
                &board,
                false,
                festival_core::constants::SearchMode::Normal,
                &mut helper,
                &mut state,
            );
            if let Some(other) = no_cache_state.as_mut() {
                let _ = find_possible_moves(
                    &ctx,
                    &board,
                    false,
                    festival_core::constants::SearchMode::Normal,
                    &mut helper_off,
                    other,
                );
            }
        }

        println!(
            "{level_number},{level_boards},{level_moves},{level_board_diff},{level_move_diff},{level_cache_diff},{:.3}",
            level_eval.as_secs_f64()
        );
    }

    println!();
    println!(
        "boards {total_boards}, moves {total_moves}, board mismatches {board_mismatches}, move mismatches {move_mismatches}"
    );
    println!(
        "false positives (rust deadlocked only) {false_positives}, false negatives {false_negatives}"
    );
    if cache_off {
        println!("cache-off verdict differences {cache_diffs}");
    }
    println!(
        "rust eval {:.3}s, rust move eval {:.3}s",
        eval_time.as_secs_f64(),
        move_time.as_secs_f64()
    );
    if cpp_board_calls > 0 && cpp_move_calls > 0 {
        let rust_board_ns = eval_time.as_nanos() as f64;
        let rust_move_ns = move_time.as_nanos() as f64;
        let cpp_board_avg = cpp_board_ns as f64 / cpp_board_calls as f64;
        let cpp_move_avg = cpp_move_ns as f64 / cpp_move_calls as f64;
        let rust_board_avg = rust_board_ns / total_boards as f64 / 2.0;
        let rust_move_avg = rust_move_ns / total_moves as f64;
        println!(
            "cpp board/call {cpp_board_avg:.0} ns, rust board/call {rust_board_avg:.0} ns, ratio {:.2}",
            rust_board_avg / cpp_board_avg
        );
        println!(
            "cpp move/call {cpp_move_avg:.0} ns, rust move/call {rust_move_avg:.0} ns, ratio {:.2}",
            rust_move_avg / cpp_move_avg
        );
    }
    println!("levels with failures {failures}");

    if board_mismatches == 0 && move_mismatches == 0 && failures == 0 && cache_diffs == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn verify_solutions(levels_path: &Path, solutions_path: &Path) -> ExitCode {
    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let text = match fs::read_to_string(solutions_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{}: {err}", solutions_path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut levels: HashMap<usize, Level> = HashMap::new();
    let mut current_level: Option<usize> = None;
    let mut expect_lurd = false;
    let mut checked = 0usize;
    let mut failures = 0usize;

    println!("level,solved,moves,pushes");

    for line in text.lines() {
        if let Some(number) = line.strip_prefix("Level ") {
            current_level = number.trim().parse().ok();
            expect_lurd = false;
            continue;
        }
        if line.trim() == "Solution" {
            expect_lurd = true;
            continue;
        }
        if !expect_lurd {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        expect_lurd = false;

        let Some(number) = current_level else {
            eprintln!("solution without a level header: {line}");
            failures += 1;
            continue;
        };
        let Some(raw) = raws.get(number.saturating_sub(1)).cloned() else {
            eprintln!("level {number}: no such level");
            failures += 1;
            continue;
        };
        if let Entry::Vacant(entry) = levels.entry(number) {
            match Level::preprocess(raw) {
                Ok(level) => {
                    entry.insert(level);
                }
                Err(err) => {
                    eprintln!("level {number}: {}", err.reason());
                    failures += 1;
                    continue;
                }
            }
        }
        let level = &levels[&number];

        match lurd::replay(&level.initial, line) {
            Ok(stats) => {
                println!(
                    "{number},{},{},{}",
                    stats.solved as u8, stats.moves, stats.pushes
                );
                if !stats.solved {
                    failures += 1;
                }
            }
            Err(err) => {
                eprintln!("level {number}: invalid LURD: {err}");
                failures += 1;
            }
        }
        checked += 1;
    }

    eprintln!("checked {checked} solutions, {failures} invalid");
    if failures == 0 && checked > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
