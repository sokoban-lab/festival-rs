use std::fmt;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use festival_core::constants::MAX_INNER;
use festival_core::io::lurd;
use festival_core::io::sok::{RawLevel, load_levels, parse_levels, resolve_level_file};
use festival_core::io::solution::{
    append_level_to_solutions, append_solution, append_solution_time, append_times_row,
    write_solutions_header, write_times_footer, write_times_header,
};
use festival_core::search::FessConfig;
use festival_core::solver::{
    CORE_COUNTS, SchedulerConfig, SolveOutcome, Solver, default_cores, solve_forward,
    strategy_supported,
};
use festival_core::{Level, LevelContext, SearchClock};

mod json;
mod spec;

use spec::{LevelSpec, parse_duration, parse_level_spec, parse_memory};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const SOLVER_NAME: &str = "Festival 3.1";
const BANNER_NAME: &str = "Festival 3.1 (Rust port)";
const DEFAULT_TREE_LOG_SIZE: u32 = 20;
const MIN_TREE_LOG_SIZE: u32 = 14;
const MAX_TREE_LOG_SIZE: u32 = 23;
/// Estimated process memory per tree node, including per-worker caches.
const BYTES_PER_TREE_NODE: u64 = 256;

/// Options that are no longer accepted, with their replacements.
const REPLACED_OPTIONS: [(&str, &str); 23] = [
    ("-level", "--level <SPEC>"),
    ("-from", "--level <SPEC>"),
    ("-to", "--level <SPEC>"),
    ("--from", "--level <SPEC>"),
    ("--to", "--level <SPEC>"),
    ("-out_dir", "--output-dir"),
    ("--out_dir", "--output-dir"),
    ("--out-dir", "--output-dir"),
    ("-out_file", "--output-file"),
    ("--out_file", "--output-file"),
    ("--out-file", "--output-file"),
    ("-time", "--timeout"),
    ("--time", "--timeout"),
    ("-save_best", "--save-best"),
    ("--save_best", "--save-best"),
    ("-cores", "--threads"),
    ("--cores", "--threads"),
    ("-alg", "--algorithm"),
    ("--alg", "--algorithm"),
    ("-extra_mem", "--memory"),
    ("--extra_mem", "--memory"),
    ("--extra-mem", "--memory"),
    ("--tree-log-size", "--memory"),
];

#[derive(Debug, Parser)]
#[command(
    name = "festival-rs",
    version,
    about = "Festival 3.1 Rust port: solve Sokoban level sets with eight algorithms.",
    after_help = "\
Examples:
  festival-rs levels/corpus/microban.xsb
  festival-rs levels/corpus/microban.xsb -l 1..20 -t 60
  festival-rs levels/corpus/xsokoban.sok -j 1 --memory 2G
  festival-rs levels/corpus/xsokoban.sok --analyse --dump
  festival-rs levels/smoke/smoke.sok --format json

Output files default to the current directory; use -o DIR to change that."
)]
struct Args {
    /// Level set file, a name resolved under `<output-dir>/levels/<name>.sok`, or
    /// `-` to read from standard input.
    #[arg(value_name = "INPUT")]
    input: String,

    /// Levels to solve, 1-based: `all`, `3`, `1..5`, or `2,7,9`.
    #[arg(
        short,
        long,
        value_name = "SPEC",
        default_value = "all",
        value_parser = parse_level_spec,
        help_heading = "Search options"
    )]
    level: LevelSpec,

    /// Time limit per level; units `ms`, `s`, `m`, `h`; a bare number is seconds.
    #[arg(
        short,
        long,
        value_name = "DURATION",
        default_value = "600",
        value_parser = parse_duration,
        help_heading = "Search options"
    )]
    timeout: Duration,

    /// Force one algorithm `a`..`h` (or `0`..`7`); the default runs all eight.
    #[arg(
        short,
        long,
        value_name = "ALGORITHM",
        value_enum,
        ignore_case = true,
        help_heading = "Search options"
    )]
    algorithm: Option<Strategy>,

    /// Worker threads: 1/2/4/8. Defaults to the host processor count, capped.
    #[arg(short = 'j', long, value_name = "N", help_heading = "Search options")]
    threads: Option<usize>,

    /// Approximate memory budget for the search, e.g. `512M` or `2G`; split
    /// across worker threads. A bare number is MiB. Defaults to about 256 MiB
    /// per worker.
    #[arg(
        short,
        long,
        value_name = "SIZE",
        value_parser = parse_memory,
        help_heading = "Search options"
    )]
    memory: Option<u64>,

    /// Disable the time limit and stop after N node expansions.
    #[arg(long, value_name = "N", help_heading = "Search options")]
    max_expansions: Option<u64>,

    /// Skip reverse packing and search forward from the initial position.
    #[arg(long, help_heading = "Search options")]
    forward_only: bool,

    /// Keep the best partial solution when a level is not solved.
    #[arg(long, help_heading = "Search options")]
    save_best: bool,

    /// Replay and validate every produced solution.
    #[arg(long, help_heading = "Search options")]
    validate: bool,

    /// Write `solutions.sok` and `times.txt` here; short input names resolve
    /// under `<DIR>/levels/`.
    #[arg(
        short,
        long,
        value_name = "DIR",
        default_value = ".",
        help_heading = "Output options"
    )]
    output_dir: PathBuf,

    /// Write only this solution file and skip `times.txt` (YASC mode).
    #[arg(long, value_name = "FILE", help_heading = "Output options")]
    output_file: Option<PathBuf>,

    /// Output format.
    #[arg(
        long,
        value_name = "FORMAT",
        value_enum,
        default_value = "human",
        help_heading = "Output options"
    )]
    format: Format,

    /// Print a static analysis summary instead of solving.
    #[arg(long, help_heading = "Analysis options")]
    analyse: bool,

    /// Dump the initial board and room decomposition (requires `--analyse`).
    #[arg(long, help_heading = "Analysis options")]
    dump: bool,
}

/// The output format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Format {
    /// Human readable output.
    Human,
    /// Machine readable JSON output.
    Json,
}

/// A forced strategy; `a` is 0 (strategy A) and `h` is 7 (strategy H).
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Strategy {
    #[value(alias = "0")]
    A,
    #[value(alias = "1")]
    B,
    #[value(alias = "2")]
    C,
    #[value(alias = "3")]
    D,
    #[value(alias = "4")]
    E,
    #[value(alias = "5")]
    F,
    #[value(alias = "6")]
    G,
    #[value(alias = "7")]
    H,
}

impl Strategy {
    fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::H => "H",
        })
    }
}

fn main() -> ExitCode {
    if let Some((name, replacement)) = replaced_option() {
        eprintln!("error: `{name}` was replaced; use `{replacement}` instead");
        return ExitCode::from(2);
    }
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn replaced_option() -> Option<(&'static str, &'static str)> {
    for arg in std::env::args_os().skip(1) {
        let Some(arg) = arg.to_str() else {
            continue;
        };
        if arg == "--" {
            break;
        }
        if let Some(entry) = replacement_for(arg) {
            return Some(entry);
        }
    }
    None
}

fn replacement_for(arg: &str) -> Option<(&'static str, &'static str)> {
    let option = arg.split_once('=').map_or(arg, |(option, _)| option);
    REPLACED_OPTIONS
        .iter()
        .copied()
        .find(|(name, _)| *name == option)
}

fn run(args: &Args) -> Result<(), String> {
    if let Some(threads) = args.threads {
        if !CORE_COUNTS.contains(&threads) {
            return Err("number of threads must be 1/2/4/8".to_string());
        }
    }
    if let Some(algorithm) = args.algorithm {
        if !strategy_supported(algorithm.index()) {
            return Err(format!("algorithm {algorithm} is not implemented"));
        }
    }

    let (raws, input_name) = load_input(args)?;
    let selected = select_levels(args, raws.len())?;
    let human = args.format == Format::Human;

    if human {
        println!("===================================");
        println!("{BANNER_NAME}");
        println!("===================================");
        println!("{} levels loaded from {input_name}", raws.len());
    }

    if args.analyse {
        return run_analysis(args, raws, &selected);
    }

    let threads = args.threads.unwrap_or_else(default_cores);
    let search_threads = if args.forward_only { 1 } else { threads };
    let log_max_nodes = tree_log_size(args.memory, search_threads)?;
    if human {
        if args.forward_only {
            println!("note: --forward-only runs the Phase 2 NORMAL search on one thread.");
        } else if let Some(algorithm) = args.algorithm {
            println!("using {threads} thread(s); forced algorithm {algorithm}");
        } else {
            println!("using {threads} thread(s); scheduling algorithms A-H");
        }
        if let Some(memory) = args.memory {
            println!("memory budget: {} MiB", memory / (1024 * 1024));
        }
    }

    run_solver(args, raws, &selected, threads, log_max_nodes)
}

fn load_input(args: &Args) -> Result<(Vec<RawLevel>, String), String> {
    if args.input == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|err| format!("stdin: {err}"))?;
        let raws = parse_levels(&text);
        if raws.is_empty() {
            return Err("no levels in stdin".to_string());
        }
        Ok((raws, "stdin".to_string()))
    } else {
        let path = resolve_level_file(&args.output_dir, &args.input);
        let raws = load_levels(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        Ok((raws, path.display().to_string()))
    }
}

fn select_levels(args: &Args, total: usize) -> Result<Vec<usize>, String> {
    if total == 0 {
        return Err("no levels in input".to_string());
    }
    args.level
        .select(total)
        .map_err(|err| format!("{err} (input has {total} levels)"))
}

/// Per-level outcome normalized across the scheduler and `--forward-only`.
struct LevelResult {
    solved: bool,
    failure_reason: Option<&'static str>,
    solutions: Vec<SolveOutcome>,
    reported: Option<SolveOutcome>,
    expansions: u64,
    elapsed: Duration,
}

fn solution_list(outcomes: &[SolveOutcome]) -> Vec<SolveOutcome> {
    outcomes
        .iter()
        .filter(|outcome| !outcome.lurd.is_empty())
        .cloned()
        .collect()
}

fn run_solver(
    args: &Args,
    raws: Vec<RawLevel>,
    selected: &[usize],
    threads: usize,
    log_max_nodes: u32,
) -> Result<(), String> {
    let human = args.format == Format::Human;
    let yasc_mode = args.output_file.is_some();
    let solution_path = match &args.output_file {
        Some(path) => path.clone(),
        None => args.output_dir.join("solutions.sok"),
    };
    let times_path = args.output_dir.join("times.txt");

    if !yasc_mode {
        fs::create_dir_all(&args.output_dir)
            .map_err(|err| format!("{}: {err}", args.output_dir.display()))?;
    } else if let Some(parent) = solution_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
        }
    }

    write_solutions_header(&solution_path, &args.input, SOLVER_NAME)
        .map_err(|err| format!("{}: {err}", solution_path.display()))?;
    if !yasc_mode {
        write_times_header(&times_path, SOLVER_NAME)
            .map_err(|err| format!("{}: {err}", times_path.display()))?;
    }

    // One worker set for the whole run, like C++ `search_trees[]`.
    let mut solver = Solver::new();
    let mut record = 0usize;
    let mut total_levels = 0usize;
    let mut solved_levels = 0usize;
    let mut total_moves = 0usize;
    let mut total_pushes = 0usize;
    let mut total_elapsed = Duration::ZERO;
    let mut results = Vec::new();

    for (index, raw) in raws.into_iter().enumerate() {
        if !selected.contains(&index) {
            continue;
        }
        let level_number = index + 1;

        record += 1;
        total_levels += 1;
        let level_started = Instant::now();
        let level_epoch = epoch_secs();
        let title = raw.title.clone();

        if human {
            println!();
            println!("Level {level_number}: {title}");
        }

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
                if human {
                    println!("  failed: {}", err.reason());
                } else {
                    results.push(failed_result(
                        level_number,
                        &title,
                        err.reason(),
                        level_started.elapsed(),
                    ));
                }
                continue;
            }
        };

        let analysis_clock = SearchClock::new(Some(args.timeout), None);
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
                if human {
                    println!("  analysis failed: {}", err.reason());
                } else {
                    results.push(failed_result(
                        level_number,
                        &title,
                        err.reason(),
                        level_started.elapsed(),
                    ));
                }
                continue;
            }
        };

        // C++ budgets with `time(0)`; compare epoch seconds so the remaining
        // whole-second count matches even when preprocessing straddles a tick.
        // Sub-second budgets have no C++ equivalent, so they use the clock.
        let remaining = if args.timeout >= Duration::from_secs(1) {
            let elapsed_secs = epoch_secs().saturating_sub(level_epoch);
            args.timeout
                .saturating_sub(Duration::from_secs(elapsed_secs))
        } else {
            args.timeout.saturating_sub(level_started.elapsed())
        };

        let result = if args.forward_only {
            let config = FessConfig {
                time_limit: if args.max_expansions.is_some() {
                    None
                } else {
                    Some(remaining)
                },
                max_expansions: args.max_expansions,
                log_max_nodes,
                save_best: args.save_best,
                cancel: None,
            };
            match solve_forward(&ctx, &config) {
                Ok(outcome) => {
                    let expansions = outcome.expansions;
                    let elapsed = outcome.elapsed;
                    let solutions = solution_list(std::slice::from_ref(&outcome));
                    LevelResult {
                        solved: outcome.solved,
                        failure_reason: outcome.failure_reason,
                        solutions,
                        reported: Some(outcome),
                        expansions,
                        elapsed,
                    }
                }
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
                    if human {
                        println!("  solve failed: {}", err.reason());
                    } else {
                        results.push(failed_result(
                            level_number,
                            &title,
                            err.reason(),
                            level_started.elapsed(),
                        ));
                    }
                    continue;
                }
            }
        } else {
            let config = SchedulerConfig {
                cores: threads,
                forced_alg: args.algorithm.map(Strategy::index),
                time_limit: remaining,
                max_expansions: args.max_expansions,
                log_max_nodes,
                save_best: args.save_best,
            };
            match solver.solve_level(&ctx, &config) {
                Ok(run) => {
                    let solutions: Vec<SolveOutcome> =
                        run.solutions(args.save_best).into_iter().cloned().collect();
                    LevelResult {
                        solved: run.solved,
                        failure_reason: run.failure_reason,
                        solutions,
                        reported: run.reported(args.save_best).cloned(),
                        expansions: run.expansions,
                        elapsed: run.elapsed,
                    }
                }
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
                    if human {
                        println!("  solve failed: {}", err.reason());
                    } else {
                        results.push(failed_result(
                            level_number,
                            &title,
                            err.reason(),
                            level_started.elapsed(),
                        ));
                    }
                    continue;
                }
            }
        };

        if args.validate {
            for solution in &result.solutions {
                if solution.lurd.is_empty() {
                    continue;
                }
                let stats = lurd::replay(&ctx.level.initial, &solution.lurd)
                    .map_err(|err| format!("level {level_number}: {err}"))?;
                if solution.solved && !stats.solved {
                    return Err(format!(
                        "level {level_number}: independent LURD validator rejected the solution"
                    ));
                }
            }
        }

        for solution in &result.solutions {
            if !solution.lurd.is_empty() {
                append_solution(&solution_path, &solution.lurd)
                    .map_err(|err| format!("{}: {err}", solution_path.display()))?;
            }
        }
        append_solution_time(&solution_path, level_started.elapsed(), SOLVER_NAME)
            .map_err(|err| format!("{}: {err}", solution_path.display()))?;

        let failure_reason = if result.solved {
            None
        } else {
            let mut reason = result
                .failure_reason
                .unwrap_or("Unknown reason")
                .to_string();
            if args.max_expansions.is_none()
                && level_started.elapsed() >= args.timeout
                && reason != "Too many moves"
            {
                reason = "Time limit exceeded".to_string();
            }
            Some(reason)
        };
        let (moves, pushes) = if result.solved {
            result
                .reported
                .as_ref()
                .map(|outcome| (outcome.moves, outcome.pushes))
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        if !yasc_mode {
            append_times_row(
                &times_path,
                record,
                level_started.elapsed(),
                SOLVER_NAME,
                result.solved,
                failure_reason.as_deref().unwrap_or("Unknown reason"),
                moves,
                pushes,
                &title,
            )
            .map_err(|err| format!("{}: {err}", times_path.display()))?;
        }

        if result.solved {
            solved_levels += 1;
            total_moves += moves;
            total_pushes += pushes;
            if human {
                println!(
                    "  SOLVED: {} moves ({} pushes), {} expansions, {:.3}s, {} solution(s)",
                    moves,
                    pushes,
                    result.expansions,
                    result.elapsed.as_secs_f64(),
                    result.solutions.len().max(1)
                );
            }
        } else if human {
            println!(
                "  not solved: {} ({} expansions, {:.3}s)",
                failure_reason.as_deref().unwrap_or("Unknown reason"),
                result.expansions,
                result.elapsed.as_secs_f64()
            );
        }
        total_elapsed += level_started.elapsed();

        if !human {
            results.push(json::SolveResult {
                index: level_number,
                title,
                status: if result.solved { "solved" } else { "unsolved" },
                failure: failure_reason,
                moves,
                pushes,
                expansions: result.expansions,
                elapsed_ms: json::millis(level_started.elapsed()),
                solutions: result
                    .solutions
                    .iter()
                    .map(|solution| json::Solution {
                        lurd: solution.lurd.clone(),
                        solved: solution.solved,
                        moves: solution.moves,
                        pushes: solution.pushes,
                    })
                    .collect(),
            });
        }
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
    }

    if human {
        println!();
        if yasc_mode {
            println!(
                "Solved {solved_levels}/{total_levels} levels. Solutions: {}",
                solution_path.display()
            );
        } else {
            println!(
                "Solved {solved_levels}/{total_levels} levels. Results: {}",
                times_path.display()
            );
        }
    } else {
        json::write(
            "solve",
            &json::SolveReport {
                results,
                summary: json::SolveSummary {
                    levels: total_levels,
                    solved: solved_levels,
                    moves: total_moves,
                    pushes: total_pushes,
                    elapsed_ms: json::millis(total_elapsed),
                },
            },
        )?;
    }

    Ok(())
}

fn failed_result(index: usize, title: &str, reason: &str, elapsed: Duration) -> json::SolveResult {
    json::SolveResult {
        index,
        title: title.to_string(),
        status: "failed",
        failure: Some(reason.to_string()),
        moves: 0,
        pushes: 0,
        expansions: 0,
        elapsed_ms: json::millis(elapsed),
        solutions: Vec::new(),
    }
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

fn tree_log_size(memory: Option<u64>, threads: usize) -> Result<u32, String> {
    let Some(memory) = memory else {
        return Ok(DEFAULT_TREE_LOG_SIZE);
    };
    let per_thread = memory / threads as u64;
    let nodes = per_thread / BYTES_PER_TREE_NODE;
    let min_nodes = 1u64 << MIN_TREE_LOG_SIZE;
    if nodes < min_nodes {
        let min_mib = (threads as u64 * min_nodes * BYTES_PER_TREE_NODE) / (1024 * 1024);
        return Err(format!(
            "memory budget is too small for {threads} thread(s); use at least {min_mib} MiB"
        ));
    }
    Ok(nodes.ilog2().min(MAX_TREE_LOG_SIZE))
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn run_analysis(args: &Args, raws: Vec<RawLevel>, selected: &[usize]) -> Result<(), String> {
    let human = args.format == Format::Human;
    let mut results = Vec::new();
    let mut total_elapsed = Duration::ZERO;

    for (index, raw) in raws.into_iter().enumerate() {
        if !selected.contains(&index) {
            continue;
        }
        let level_number = index + 1;
        let title = raw.title.clone();
        let started = Instant::now();
        let clock = SearchClock::new(Some(args.timeout), args.max_expansions);
        let level = match Level::preprocess(raw) {
            Ok(level) => level,
            Err(err) => {
                total_elapsed += started.elapsed();
                if human {
                    println!("Level {level_number}: {title}");
                    println!("  failed: {}", err.reason());
                } else {
                    results.push(failed_analysis(level_number, title, err.reason()));
                }
                continue;
            }
        };

        match LevelContext::analyse(level, &clock) {
            Ok(context) => {
                let details = analysis_details(&context, &clock);
                if human {
                    report_level(level_number, &context, args.dump, &details);
                } else {
                    results.push(json::AnalysisResult {
                        index: level_number,
                        title,
                        status: "ok",
                        failure: None,
                        analysis: Some(details),
                    });
                }
                total_elapsed += started.elapsed();
            }
            Err(err) => {
                total_elapsed += started.elapsed();
                if human {
                    println!("Level {level_number}: {title}");
                    println!("  analysis failed: {}", err.reason());
                } else {
                    results.push(failed_analysis(level_number, title, err.reason()));
                }
            }
        }
    }

    if !human {
        json::write(
            "analyse",
            &json::AnalysisReport {
                summary: json::AnalysisSummary {
                    levels: results.len(),
                    elapsed_ms: json::millis(total_elapsed),
                },
                results,
            },
        )?;
    }
    Ok(())
}

fn failed_analysis(index: usize, title: String, reason: &str) -> json::AnalysisResult {
    json::AnalysisResult {
        index,
        title,
        status: "failed",
        failure: Some(reason.to_string()),
        analysis: None,
    }
}

fn analysis_details(context: &LevelContext, clock: &SearchClock) -> json::AnalysisDetails {
    let level = &context.level;
    let targets = level
        .initial
        .cells()
        .iter()
        .take(MAX_INNER)
        .filter(|cell| **cell & festival_core::constants::TARGET != 0)
        .count();

    json::AnalysisDetails {
        width: level.initial.width(),
        height: level.initial.height(),
        boxes: level.box_count,
        targets,
        inner_cells: level.index_num(),
        rooms: context.rooms.num,
        hotspots: context.hotspots.score(level, &level.board),
        aborted_hotspots: context.hotspots.aborted,
        forbidden_push: level
            .forbidden_push
            .cells()
            .iter()
            .take(MAX_INNER)
            .filter(|v| **v != 0)
            .count(),
        forbidden_pull: level
            .forbidden_pull
            .cells()
            .iter()
            .take(MAX_INNER)
            .filter(|v| **v != 0)
            .count(),
        hash: format!("0x{:016x}", level.initial.hash()),
        elapsed_ms: json::millis(clock.elapsed()),
    }
}

fn report_level(
    level_number: usize,
    context: &LevelContext,
    dump: bool,
    details: &json::AnalysisDetails,
) {
    let level = &context.level;

    println!();
    println!("Level {level_number}: {}", level.title);
    println!(
        "  size: {}x{}, boxes: {}, targets: {}, inner cells: {}, rooms: {}",
        details.width,
        details.height,
        details.boxes,
        details.targets,
        details.inner_cells,
        details.rooms
    );
    println!(
        "  hotspots: {} (aborted: {}), forbidden push/pull cells: {}/{}",
        details.hotspots, details.aborted_hotspots, details.forbidden_push, details.forbidden_pull
    );
    println!("  initial hash: {}", details.hash);
    println!(
        "  push distances: {}x{}, BFS distances: {}x{}, analysis: {:.3} ms",
        details.inner_cells,
        details.inner_cells,
        details.inner_cells,
        details.inner_cells,
        details.elapsed_ms
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

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn maps_replaced_options() {
        assert_eq!(replacement_for("--time=60"), Some(("--time", "--timeout")));
        assert_eq!(replacement_for("-cores"), Some(("-cores", "--threads")));
        assert_eq!(replacement_for("--alg=A"), Some(("--alg", "--algorithm")));
        assert_eq!(
            replacement_for("--out-dir=out"),
            Some(("--out-dir", "--output-dir"))
        );
        assert_eq!(
            replacement_for("--tree-log-size=21"),
            Some(("--tree-log-size", "--memory"))
        );
        assert_eq!(
            replacement_for("--extra-mem=2"),
            Some(("--extra-mem", "--memory"))
        );
        assert_eq!(replacement_for("--level=1..5"), None);
        assert_eq!(replacement_for("levels.sok"), None);
    }

    #[test]
    fn maps_memory_to_tree_log_size() {
        let mib = 1024 * 1024;
        assert_eq!(tree_log_size(None, 1).unwrap(), DEFAULT_TREE_LOG_SIZE);
        assert_eq!(tree_log_size(Some(256 * mib), 1).unwrap(), 20);
        assert_eq!(tree_log_size(Some(512 * mib), 1).unwrap(), 21);
        assert_eq!(tree_log_size(Some(1024 * mib), 1).unwrap(), 22);
        assert_eq!(tree_log_size(Some(2048 * mib), 1).unwrap(), 23);
        assert_eq!(tree_log_size(Some(4096 * mib), 1).unwrap(), 23);
        assert_eq!(tree_log_size(Some(512 * mib), 4).unwrap(), 19);
        assert_eq!(tree_log_size(Some(512 * mib), 8).unwrap(), 18);
        assert!(tree_log_size(Some(16 * mib), 8).is_err());
    }

    #[test]
    fn cli_definition_is_valid() {
        Args::command().debug_assert();
    }
}
