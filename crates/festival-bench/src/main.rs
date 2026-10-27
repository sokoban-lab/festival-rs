// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! End-to-end driver for static analysis and differential baselines.
//!
//! Usage: `festival-bench [levels.sok]`

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use festival_core::io::sok::{load_levels, resolve_level_file};
use festival_core::{Level, LevelContext, SearchClock};

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let name = args
        .next()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "XSokoban".to_string());

    let path = resolve_level_file(&PathBuf::from("."), &name);
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
