// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use festival_core::constants::MAX_INNER;
use festival_core::io::sok::{load_levels, resolve_level_file};
use festival_core::{Level, LevelContext, SearchClock};

#[derive(Debug, Parser)]
#[command(
    name = "festival-rs",
    about = "Festival 3.1 Rust port. This build contains Phase 1 static analysis.",
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

    /// Skip the reverse packing search (development only).
    #[arg(long = "forward-only")]
    forward_only: bool,

    /// Replay and validate every produced solution (Phase 2+).
    #[arg(long = "validate")]
    validate: bool,

    /// Print a static analysis summary (default in this build).
    #[arg(long = "analyse")]
    analyse: bool,

    /// Dump boards, masks and room decomposition.
    #[arg(long = "dump")]
    dump: bool,
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
    }
    if args.out_file.is_some() || args.save_best || args.forward_only || args.validate {
        eprintln!(
            "note: -out_file/-save_best/--forward-only/--validate are accepted for compatibility; \
             search phases are not implemented in this build yet"
        );
    }
    let _ = (args.alg, args.extra_mem, args.max_expansions, args.analyse);

    let path = resolve_level_file(&args.out_dir, &args.levels);
    let raws = load_levels(&path).map_err(|err| format!("{}: {err}", path.display()))?;

    let (first, last) = select_range(args, raws.len())?;

    println!("===================================");
    println!("Festival 3.1 (Rust port) - Phase 1 static analysis");
    println!("Derived from Festival 3.1, Copyright 2018-2022 Yaron Shoham");
    println!("===================================");
    println!("{} levels loaded from {}", raws.len(), path.display());

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
