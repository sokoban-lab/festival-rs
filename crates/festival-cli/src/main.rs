// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Parser;
use festival_core::constants::MAX_INNER;
use festival_core::io::sok::{RawLevel, load_levels, resolve_level_file};
use festival_core::io::solution::{
    append_level_to_solutions, append_solution, append_solution_time, append_times_row,
    write_solutions_header, write_times_footer, write_times_header,
};
use festival_core::search::FessConfig;
use festival_core::solver::{StrategyConfig, solve_forward, solve_strategy, strategy_supported};
use festival_core::{Level, LevelContext, SearchClock};

const SOLVER_NAME: &str = "Festival 3.1";
const BANNER_NAME: &str = "Festival 3.1 (Rust port)";
const BASE_TREE_LOG_SIZE: u32 = 20;

#[derive(Debug, Parser)]
#[command(
    name = "festival-rs",
    about = "Festival 3.1 Rust port. Phase 4: reverse packing, planning and HF strategies.",
    disable_help_subcommand = true
)]
struct Args {
    /// Level set file, or a name resolved under `<out_dir>/levels/<name>.sok`.
    #[arg(value_name = "levels.sok")]
    levels: String,

    #[arg(long = "level", value_name = "N")]
    level: Option<usize>,

    #[arg(long = "from", value_name = "N")]
    from: Option<usize>,

    #[arg(long = "to", value_name = "N")]
    to: Option<usize>,

    #[arg(long = "out_dir", value_name = "DIR", default_value = ".")]
    out_dir: PathBuf,

    #[arg(long = "time", value_name = "SEC", default_value_t = 600)]
    time: u64,

    #[arg(long = "out_file", value_name = "FILE")]
    out_file: Option<PathBuf>,

    #[arg(long = "save_best")]
    save_best: bool,

    #[arg(long = "cores", value_name = "N")]
    cores: Option<usize>,

    #[arg(long = "alg", value_name = "N")]
    alg: Option<i32>,

    #[arg(long = "extra_mem", value_name = "N", default_value_t = 0)]
    extra_mem: i32,

    /// Disable the time limit and stop after N expansions (differential mode).
    #[arg(long = "max-expansions", value_name = "N")]
    max_expansions: Option<u64>,

    /// Skip the reverse packing search and run NORMAL FESS from the initial
    /// position (development/differential mode).
    #[arg(long = "forward-only")]
    forward_only: bool,

    /// Replay and validate every produced solution.
    #[arg(long = "validate")]
    validate: bool,

    /// Print a static analysis summary instead of solving.
    #[arg(long = "analyse")]
    analyse: bool,

    /// Dump boards, masks and room decomposition (with --analyse).
    #[arg(long = "dump")]
    dump: bool,

    /// Tree capacity as `log2(nodes)`; defaults to 20 and can be raised for
    /// large levels. `-extra_mem` is added on top.
    #[arg(long = "tree-log-size", value_name = "N")]
    tree_log_size: Option<u32>,
}

fn main() -> ExitCode {
    let args = Args::parse_from(normalized_args());
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn normalized_args() -> Vec<OsString> {
    const SINGLE_DASH: [(&str, &str); 10] = [
        ("-level", "--level"),
        ("-from", "--from"),
        ("-to", "--to"),
        ("-out_dir", "--out_dir"),
        ("-time", "--time"),
        ("-out_file", "--out_file"),
        ("-save_best", "--save_best"),
        ("-cores", "--cores"),
        ("-alg", "--alg"),
        ("-extra_mem", "--extra_mem"),
    ];

    std::env::args_os()
        .map(|arg| {
            if let Some(text) = arg.to_str() {
                for (from, to) in SINGLE_DASH {
                    if text == from {
                        return OsString::from(to);
                    }
                }
            }
            arg
        })
        .collect()
}

fn run(args: &Args) -> Result<(), String> {
    if let Some(cores) = args.cores {
        if !matches!(cores, 1 | 2 | 4 | 8) {
            return Err("Number of cores should be 1/2/4/8".to_string());
        }
    }
    if let Some(alg) = args.alg {
        if !(0..=7).contains(&alg) {
            return Err("Algorithm number should be 0..7".to_string());
        }
        if !strategy_supported(alg as usize) {
            return Err(format!("strategy {alg} is not implemented"));
        }
    }

    let path = resolve_level_file(&args.out_dir, &args.levels);
    let raws = load_levels(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    let (first, last) = select_range(args, raws.len())?;

    println!("===================================");
    println!("{BANNER_NAME}");
    println!("Copyright 2019-2022 by Yaron Shoham");
    println!("===================================");
    println!("{} levels loaded from {}", raws.len(), path.display());

    if args.analyse {
        return run_analysis(args, raws, first, last);
    }

    if !args.forward_only && args.alg.is_none() {
        println!(
            "note: runs strategy A by default; full 8-strategy scheduling arrives in Phase 6."
        );
    }
    if let Some(cores) = args.cores {
        if cores > 1 {
            println!("note: Phase 4 is single-threaded; ignoring -cores {cores}.");
        }
    }
    let _ = args.validate;

    run_solver(args, raws, first, last)
}

fn run_solver(args: &Args, raws: Vec<RawLevel>, first: usize, last: usize) -> Result<(), String> {
    let yasc_mode = args.out_file.is_some();
    let solution_path = match &args.out_file {
        Some(path) => path.clone(),
        None => args.out_dir.join("solutions.sok"),
    };
    let times_path = args.out_dir.join("times.txt");

    if !yasc_mode {
        fs::create_dir_all(&args.out_dir)
            .map_err(|err| format!("{}: {err}", args.out_dir.display()))?;
    } else if let Some(parent) = solution_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
        }
    }

    write_solutions_header(&solution_path, &args.levels, SOLVER_NAME)
        .map_err(|err| format!("{}: {err}", solution_path.display()))?;
    if !yasc_mode {
        write_times_header(&times_path, SOLVER_NAME)
            .map_err(|err| format!("{}: {err}", times_path.display()))?;
    }

    let log_max_nodes = tree_log_size(args);
    let mut record = 0usize;
    let mut total_levels = 0usize;
    let mut solved_levels = 0usize;
    let mut total_moves = 0usize;
    let mut total_pushes = 0usize;
    let mut total_elapsed = Duration::ZERO;

    for (index, raw) in raws.into_iter().enumerate() {
        let level_number = index + 1;
        if level_number < first || level_number > last {
            continue;
        }

        record += 1;
        total_levels += 1;
        let level_started = Instant::now();
        let title = raw.title.clone();

        println!();
        println!("Level {level_number}: {title}");

        if let Some(initial) = raw.board {
            append_level_to_solutions(&solution_path, level_number, &title, &initial)
                .map_err(|err| format!("{}: {err}", solution_path.display()))?;
        }

        let level = match Level::preprocess(raw) {
            Ok(level) => level,
            Err(err) => {
                record_failure(
                    yasc_mode,
                    &times_path,
                    record,
                    level_started.elapsed(),
                    err.reason(),
                    &title,
                )?;
                total_elapsed += level_started.elapsed();
                println!("  failed: {}", err.reason());
                continue;
            }
        };

        let analysis_clock = SearchClock::new(Some(Duration::from_secs(args.time)), None);
        let ctx = match LevelContext::analyse(level, &analysis_clock) {
            Ok(ctx) => ctx,
            Err(err) => {
                record_failure(
                    yasc_mode,
                    &times_path,
                    record,
                    level_started.elapsed(),
                    err.reason(),
                    &title,
                )?;
                total_elapsed += level_started.elapsed();
                println!("  analysis failed: {}", err.reason());
                continue;
            }
        };

        let remaining = Duration::from_secs(args.time).saturating_sub(level_started.elapsed());

        let outcome = if args.forward_only {
            let config = FessConfig {
                time_limit: if args.max_expansions.is_some() {
                    None
                } else {
                    Some(remaining)
                },
                max_expansions: args.max_expansions,
                log_max_nodes,
                save_best: args.save_best,
            };
            solve_forward(&ctx, &config)
        } else {
            let alg = args.alg.unwrap_or(0) as usize;
            let config = StrategyConfig {
                time_limit: remaining,
                max_expansions: args.max_expansions,
                log_max_nodes,
                save_best: args.save_best,
            };
            solve_strategy(&ctx, alg, &config)
        };
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(err) => {
                record_failure(
                    yasc_mode,
                    &times_path,
                    record,
                    level_started.elapsed(),
                    err.reason(),
                    &title,
                )?;
                total_elapsed += level_started.elapsed();
                println!("  solve failed: {}", err.reason());
                continue;
            }
        };

        if !outcome.lurd.is_empty() {
            append_solution(&solution_path, &outcome.lurd)
                .map_err(|err| format!("{}: {err}", solution_path.display()))?;
        }
        append_solution_time(&solution_path, level_started.elapsed(), SOLVER_NAME)
            .map_err(|err| format!("{}: {err}", solution_path.display()))?;

        let failure_reason = outcome.failure_reason.unwrap_or("Unknown reason");
        if !yasc_mode {
            append_times_row(
                &times_path,
                record,
                level_started.elapsed(),
                SOLVER_NAME,
                outcome.solved,
                failure_reason,
                if outcome.solved { outcome.moves } else { 0 },
                if outcome.solved { outcome.pushes } else { 0 },
                &title,
            )
            .map_err(|err| format!("{}: {err}", times_path.display()))?;
        }

        if outcome.solved {
            solved_levels += 1;
            total_moves += outcome.moves;
            total_pushes += outcome.pushes;
            println!(
                "  SOLVED: {} moves ({} pushes), {} expansions, {:.3}s",
                outcome.moves,
                outcome.pushes,
                outcome.expansions,
                outcome.elapsed.as_secs_f64()
            );
        } else {
            println!(
                "  not solved: {} ({} expansions, {:.3}s)",
                failure_reason,
                outcome.expansions,
                outcome.elapsed.as_secs_f64()
            );
        }
        total_elapsed += level_started.elapsed();
    }

    if !yasc_mode {
        write_times_footer(
            &times_path,
            total_levels,
            solved_levels,
            total_moves,
            total_pushes,
            total_elapsed,
        )
        .map_err(|err| format!("{}: {err}", times_path.display()))?;
        println!();
        println!(
            "Solved {solved_levels}/{total_levels} levels. Results: {}",
            times_path.display()
        );
    } else {
        println!();
        println!(
            "Solved {solved_levels}/{total_levels} levels. Solutions: {}",
            solution_path.display()
        );
    }

    Ok(())
}

fn record_failure(
    yasc_mode: bool,
    times_path: &Path,
    record: usize,
    elapsed: Duration,
    reason: &str,
    title: &str,
) -> Result<(), String> {
    if yasc_mode {
        return Ok(());
    }
    append_times_row(
        times_path,
        record,
        elapsed,
        SOLVER_NAME,
        false,
        reason,
        0,
        0,
        title,
    )
    .map_err(|err| format!("{}: {err}", times_path.display()))?;
    Ok(())
}

fn tree_log_size(args: &Args) -> u32 {
    let base = args.tree_log_size.unwrap_or(BASE_TREE_LOG_SIZE) as i32;
    (base + args.extra_mem).clamp(14, 23) as u32
}

fn run_analysis(args: &Args, raws: Vec<RawLevel>, first: usize, last: usize) -> Result<(), String> {
    for (index, raw) in raws.into_iter().enumerate() {
        let level_number = index + 1;
        if level_number < first || level_number > last {
            continue;
        }

        let clock = SearchClock::new(Some(Duration::from_secs(args.time)), args.max_expansions);
        let title = raw.title.clone();
        let level = match Level::preprocess(raw) {
            Ok(level) => level,
            Err(err) => {
                println!("Level {level_number}: {title}");
                println!("  failed: {}", err.reason());
                continue;
            }
        };

        match LevelContext::analyse(level, &clock) {
            Ok(context) => report_level(level_number, &context, &clock, args.dump),
            Err(err) => {
                println!("Level {level_number}: {title}");
                println!("  analysis failed: {}", err.reason());
            }
        }
    }

    Ok(())
}

fn select_range(args: &Args, total: usize) -> Result<(usize, usize), String> {
    if total == 0 {
        return Err("no levels in file".to_string());
    }
    if let Some(level) = args.level {
        if level == 0 || level > total {
            return Err(format!("level {level} is out of range (1..={total})"));
        }
        return Ok((level, level));
    }
    let first = args.from.unwrap_or(1);
    let last = args.to.unwrap_or(total);
    if first == 0 || last == 0 || first > last || first > total {
        return Err(format!(
            "invalid level range {first}..={last} (file has {total} levels)"
        ));
    }
    Ok((first, last.min(total)))
}

fn report_level(level_number: usize, context: &LevelContext, clock: &SearchClock, dump: bool) {
    let level = &context.level;
    let targets = level
        .initial
        .cells()
        .iter()
        .take(MAX_INNER)
        .filter(|cell| **cell & festival_core::constants::TARGET != 0)
        .count();

    println!();
    println!("Level {level_number}: {}", level.title);
    println!(
        "  size: {}x{}, boxes: {}, targets: {}, inner cells: {}, rooms: {}",
        level.initial.height(),
        level.initial.width(),
        level.box_count,
        targets,
        level.index_num(),
        context.rooms.num
    );
    println!(
        "  hotspots: {} (aborted: {}), forbidden push/pull cells: {}/{}",
        context.hotspots.score(level, &level.board),
        context.hotspots.aborted,
        level
            .forbidden_push
            .cells()
            .iter()
            .take(MAX_INNER)
            .filter(|v| **v != 0)
            .count(),
        level
            .forbidden_pull
            .cells()
            .iter()
            .take(MAX_INNER)
            .filter(|v| **v != 0)
            .count()
    );
    println!("  initial hash: 0x{:016x}", level.initial.hash());
    println!(
        "  push distances: {}x{}, BFS distances: {}x{}, analysis: {:.3} ms",
        level.index_num(),
        level.index_num(),
        level.index_num(),
        level.index_num(),
        clock.elapsed().as_secs_f64() * 1000.0
    );

    if dump {
        println!("  initial board:");
        print_indented(&level.initial.display(), "    ");
        if context.rooms.num > 0 {
            println!("  room map:");
            print_room_map(context);
        }
    }
}

fn print_room_map(context: &LevelContext) {
    let level = &context.level;
    for y in 0..level.initial.height() {
        let mut line = String::from("    ");
        for x in 0..level.initial.width() {
            if level.initial.cell(y, x) & festival_core::constants::WALL != 0 {
                line.push('#');
                continue;
            }
            let color = context.rooms.color(y, x);
            if color == festival_core::constants::INF {
                line.push(' ');
            } else {
                line.push(char::from_digit(color % 10, 10).unwrap_or('?'));
            }
        }
        println!("{line}");
    }
}

fn print_indented(text: &str, indent: &str) {
    for line in text.lines() {
        println!("{indent}{line}");
    }
}
