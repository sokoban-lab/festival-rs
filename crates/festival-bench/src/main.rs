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

use festival_core::io::lurd;
use festival_core::io::sok::{load_levels, resolve_level_file};
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

    analyse(args.first().map(String::as_str).unwrap_or("XSokoban"))
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
