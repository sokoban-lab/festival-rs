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
use festival_core::search::ScoreElement;
use festival_core::search::helper::Helper;
use festival_core::search::snail::{SnailParams, fill_netlock_scores, fill_snail_scores};
use festival_core::{Level, LevelContext, SearchClock};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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

    if args.first().map(String::as_str) == Some("dragonfly-stats") {
        if args.len() != 4 && args.len() != 5 {
            eprintln!(
                "usage: festival-bench dragonfly-stats <levels.sok> <level> <seconds> [--dump]"
            );
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        let seconds: u64 = args[3].parse().unwrap_or(0);
        return dragonfly_stats(Path::new(&args[1]), level_number, seconds);
    }

    if args.first().map(String::as_str) == Some("girl-trace") {
        if args.len() != 6 {
            eprintln!(
                "usage: festival-bench girl-trace <levels.sok> <level> <plan.txt> <boards> <out>"
            );
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        return girl_trace(
            Path::new(&args[1]),
            level_number,
            Path::new(&args[3]),
            Path::new(&args[4]),
            Path::new(&args[5]),
        );
    }

    if args.first().map(String::as_str) == Some("soak") {
        if args.len() != 5 {
            eprintln!("usage: festival-bench soak <levels.sok> <seconds> <max_expansions> <cores>");
            return ExitCode::FAILURE;
        }
        let seconds: u64 = args[2].parse().unwrap_or(0);
        let max_expansions: u64 = args[3].parse().unwrap_or(5000);
        let cores: usize = args[4].parse().unwrap_or(1);
        return soak(Path::new(&args[1]), seconds, max_expansions, cores);
    }

    if args.first().map(String::as_str) == Some("wallify-dump") {
        if args.len() != 5 {
            eprintln!("usage: festival-bench wallify-dump <levels.sok> <level> <boards> <out>");
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        return wallify_dump(
            Path::new(&args[1]),
            level_number,
            Path::new(&args[3]),
            Path::new(&args[4]),
        );
    }

    if args.first().map(String::as_str) == Some("move-dump") {
        if args.len() != 5 {
            eprintln!("usage: festival-bench move-dump <levels.sok> <level> <boards> <out>");
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        return move_dump(
            Path::new(&args[1]),
            level_number,
            Path::new(&args[3]),
            Path::new(&args[4]),
        );
    }

    if args.first().map(String::as_str) == Some("fwb-score") {
        if args.len() != 6 {
            eprintln!(
                "usage: festival-bench fwb-score <levels.sok> <level> <plan.txt> <boards> <out>"
            );
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        return fwb_score(
            Path::new(&args[1]),
            level_number,
            Path::new(&args[3]),
            Path::new(&args[4]),
            Path::new(&args[5]),
        );
    }

    if args.first().map(String::as_str) == Some("fwb-run") {
        if args.len() != 6 {
            eprintln!(
                "usage: festival-bench fwb-run <levels.sok> <level> <plan.txt> <max_expansions> <out.boards>"
            );
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        let max_expansions: u64 = args[4].parse().unwrap_or(0);
        return fwb_run(
            Path::new(&args[1]),
            level_number,
            Path::new(&args[3]),
            max_expansions,
            Path::new(&args[5]),
        );
    }

    if args.first().map(String::as_str) == Some("snail-state") {
        if args.len() != 3 {
            eprintln!("usage: festival-bench snail-state <levels.sok> <level>");
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        return snail_state(Path::new(&args[1]), level_number);
    }

    if args.first().map(String::as_str) == Some("packing-plan") {
        if args.len() != 5 {
            eprintln!(
                "usage: festival-bench packing-plan <levels.sok> <level> <strategy> <log_nodes>"
            );
            return ExitCode::FAILURE;
        }
        let level_number: usize = args[2].parse().unwrap_or(0);
        let strategy: usize = args[3].parse().unwrap_or(usize::MAX);
        let log_nodes: u32 = args[4].parse().unwrap_or(0);
        return packing_plan(Path::new(&args[1]), level_number, strategy, log_nodes);
    }

    analyse(args.first().map(String::as_str).unwrap_or("XSokoban"))
}

/// Mirrors `refcheck snail`: detection target, chain and netlock grid.
fn snail_state(levels_path: &Path, level_number: usize) -> ExitCode {
    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };

    let params = &ctx.snail;
    println!("snail {}", u8::from(params.snail_level_detected));
    println!("netlock {}", u8::from(params.netlock_level_detected));
    println!("target {} {}", params.target.0, params.target.1);
    println!("chain_len {}", params.chain_len);
    for y in 0..ctx.level.board.height() {
        for x in 0..ctx.level.board.width() {
            let index = Board::flat_index(y, x);
            if params.chain[index] {
                println!("chain {y} {x} {}", params.chain_dist[index]);
            }
        }
    }
    for y in 0..ctx.level.board.height() {
        for x in 0..ctx.level.board.width() {
            if params.netlock_grid[Board::flat_index(y, x)] {
                println!("grid {y} {x}");
            }
        }
    }

    print_snail_scores("forward", params, &ctx.level, &ctx.level.board);
    let mut reverse = ctx.level.board;
    reverse.enter_reverse_mode(&ctx.level.initial, false);
    print_snail_scores("reverse0", params, &ctx.level, &reverse);
    let mut reverse = ctx.level.board;
    reverse.enter_reverse_mode(&ctx.level.initial, true);
    print_snail_scores("reverse1", params, &ctx.level, &reverse);
    ExitCode::SUCCESS
}

/// Mirrors `refcheck snail` score rows filled by `fill_snail_scores` /
/// `fill_netlock_scores` for every alg type.
fn print_snail_scores(name: &str, params: &SnailParams, level: &Level, board: &Board) {
    for alg_type in 0..4 {
        let mut score = ScoreElement::default();
        fill_snail_scores(params, level, board, &mut score, alg_type);
        println!(
            "snail-score {name} {alg_type} {} {} {} {} {}",
            score.out_of_plan,
            score.dist_to_imagined,
            score.biconnect,
            score.hotspots,
            score.boxes_on_targets
        );

        let mut score = ScoreElement::default();
        fill_netlock_scores(params, level, board, &mut score, alg_type);
        println!(
            "netlock-score {name} {alg_type} {} {} {} {}",
            score.connectivity, score.boxes_on_targets, score.rooms_score, score.imagine
        );
    }
}

/// Mirrors `refcheck dragonfly`: Dragonfly exploration and frontier stats.
fn dragonfly_stats(levels_path: &Path, level_number: usize, seconds: u64) -> ExitCode {
    use festival_core::deadlock::state::DeadlockState;
    use festival_core::search::dragonfly::{DragonflyConfig, run_dragonfly};
    use festival_core::search::perimeter::Perimeter;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut deadlock = match DeadlockState::prepare(&ctx) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("deadlock: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut helper = Helper::default();
    let perimeter = Perimeter::new(25);
    let config = DragonflyConfig {
        time_limit: Some(Duration::from_secs(seconds)),
        trace: std::env::args().any(|arg| arg == "--dump"),
        ..DragonflyConfig::default()
    };
    let run = match run_dragonfly(
        &ctx,
        &mut deadlock,
        &mut helper,
        &perimeter,
        &config,
        &ctx.level.board,
    ) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("dragonfly: {err}");
            return ExitCode::FAILURE;
        }
    };

    println!("nodes {}", run.nodes);
    println!("roots {}", run.roots);
    println!("max_nodes {}", config.max_nodes);
    println!("perimeter_entries {}", perimeter.total_entries());
    println!("perimeter_full {}", usize::from(perimeter.is_full()));
    println!("perimeter_found {}", usize::from(helper.perimeter_found));
    if let Some(trace) = run.trace {
        for (index, node) in trace.iter().enumerate() {
            println!(
                "node {} {} {} {} {} {}",
                index, node.depth, node.father, node.dist, node.packed, node.hash
            );
        }
    }
    ExitCode::SUCCESS
}

/// Runs the FORWARD_WITH_BASES search from a fixed plan and dumps every
/// expansion board, so C++ can score/check the exact Rust nodes.
fn fwb_run(
    levels_path: &Path,
    level_number: usize,
    plan_path: &Path,
    max_expansions: u64,
    out_path: &Path,
) -> ExitCode {
    use festival_core::constants::SearchMode;
    use festival_core::planning::{oop, park_order};
    use festival_core::search::fess::{FessConfig, run_fess_into};
    use festival_core::search::perimeter::Perimeter;
    use festival_core::search::tree::Tree;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let plan = match load_plan(plan_path) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let mut helper = Helper {
        parking_order: plan,
        ..Helper::default()
    };
    if let Err(err) = park_order::verify_parking_order(&ctx, &helper) {
        eprintln!("plan: {err}");
        return ExitCode::FAILURE;
    }
    if let Err(err) = park_order::reduce_parking_order(&ctx, &mut helper) {
        eprintln!("plan: {err}");
        return ExitCode::FAILURE;
    }
    if let Err(err) = oop::prepare_oop_zones(&ctx, &mut helper) {
        eprintln!("oop: {err}");
        return ExitCode::FAILURE;
    }

    let mut tree = match Tree::new_with_level(20, &ctx.level) {
        Ok(tree) => tree,
        Err(err) => {
            eprintln!("tree: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = tree.prepare_deadlock(&ctx) {
        eprintln!("deadlock: {err}");
        return ExitCode::FAILURE;
    }
    let perimeter = Perimeter::new(25);
    let config = FessConfig {
        time_limit: None,
        max_expansions: Some(max_expansions),
        log_max_nodes: 20,
        save_best: false,
        cancel: None,
    };
    let run = match run_fess_into(
        &ctx,
        &config,
        SearchMode::ForwardWithBases,
        &mut tree,
        &mut helper,
        &perimeter,
    ) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("fess: {err}");
            return ExitCode::FAILURE;
        }
    };

    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let mut out = String::new();
    for id in 0..tree.expansions.len() {
        let Ok(board) = tree.expansion_board(id as u32) else {
            continue;
        };
        let mut hex = String::with_capacity(width * height * 2);
        for y in 0..height {
            for x in 0..width {
                hex.push_str(&format!("{:02x}", board.cell(y, x)));
            }
        }
        out.push_str(&format!("{} {hex}\n", board.hash()));
    }
    if let Err(err) = fs::write(out_path, out) {
        eprintln!("{}: {err}", out_path.display());
        return ExitCode::FAILURE;
    }
    eprintln!(
        "expansions {} solved {} nodes {}",
        run.expansions,
        run.solved,
        tree.expansions.len()
    );
    ExitCode::SUCCESS
}

/// Repeats preprocessing + solving in one process; the G7 soak driver polls
/// this process's RSS to detect leaks.
fn soak(levels_path: &Path, seconds: u64, max_expansions: u64, cores: usize) -> ExitCode {
    use festival_core::solver::scheduler::{SchedulerConfig, Solver};

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    if raws.is_empty() {
        eprintln!("no levels");
        return ExitCode::FAILURE;
    }

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut solver = Solver::new();
    let mut iterations = 0u64;
    let mut solved = 0u64;
    let mut failures = 0u64;

    while Instant::now() < deadline {
        for raw in &raws {
            let started = Instant::now();
            let level = match Level::preprocess(raw.clone()) {
                Ok(level) => level,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
            let ctx = match LevelContext::analyse(level, &clock) {
                Ok(ctx) => ctx,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            let config = SchedulerConfig {
                cores,
                forced_alg: None,
                time_limit: Duration::from_secs(600),
                max_expansions: Some(max_expansions),
                log_max_nodes: 16,
                save_best: false,
            };
            match solver.solve_level(&ctx, &config) {
                Ok(run) => {
                    if run.solved {
                        solved += 1;
                    } else if run.failure_reason == Some("Unknown reason") {
                        failures += 1;
                    }
                }
                Err(_) => failures += 1,
            }
            iterations += 1;
            if iterations % 50 == 0 {
                println!(
                    "soak iterations {iterations} solved {solved} failures {failures} last_ms {}",
                    started.elapsed().as_millis()
                );
            }
            if Instant::now() >= deadline {
                break;
            }
        }
    }

    println!("soak done iterations {iterations} solved {solved} failures {failures}");
    ExitCode::SUCCESS
}

/// Mirrors `refcheck wallify`: fixed-box wallification before move generation.
fn wallify_dump(
    levels_path: &Path,
    level_number: usize,
    boards_path: &Path,
    out_path: &Path,
) -> ExitCode {
    use festival_core::deadlock::fixed_boxes::turn_fixed_boxes_into_walls;
    use festival_core::deadlock::state::DeadlockState;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let state = match DeadlockState::prepare(&ctx) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("deadlock: {err}");
            return ExitCode::FAILURE;
        }
    };
    let boards_text = match fs::read_to_string(boards_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{}: {err}", boards_path.display());
            return ExitCode::FAILURE;
        }
    };
    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let mut out = String::new();
    for line in boards_text.lines() {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else {
            continue;
        };
        let Some(hex) = fields.next() else {
            continue;
        };
        let Some(data) = hex_board(hex, width, height) else {
            eprintln!("bad board hex on level {level_number}");
            return ExitCode::FAILURE;
        };
        let board = match Board::from_bytes(width, height, &data) {
            Ok(board) => board,
            Err(err) => {
                eprintln!("board: {err}");
                return ExitCode::FAILURE;
            }
        };
        let mut cleared = board;
        cleared.clear_deadlock_zone_inplace();
        let wallified =
            turn_fixed_boxes_into_walls(&ctx.level, &ctx.distances, &cleared, &state.envelopes);
        out.push_str(hash);
        out.push(' ');
        for y in 0..height {
            for x in 0..width {
                out.push_str(&format!("{:02x}", wallified.cell(y, x)));
            }
        }
        out.push('\n');
    }
    if let Err(err) = fs::write(out_path, out) {
        eprintln!("{}: {err}", out_path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Mirrors `refcheck moves`: forward move generation on existing boards.
fn move_dump(
    levels_path: &Path,
    level_number: usize,
    boards_path: &Path,
    out_path: &Path,
) -> ExitCode {
    use festival_core::constants::SearchMode;
    use festival_core::deadlock::state::DeadlockState;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut state = match DeadlockState::prepare(&ctx) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("deadlock: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut helper = Helper::default();
    let boards_text = match fs::read_to_string(boards_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{}: {err}", boards_path.display());
            return ExitCode::FAILURE;
        }
    };
    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let mut out = String::new();
    for line in boards_text.lines() {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else {
            continue;
        };
        let Some(hex) = fields.next() else {
            continue;
        };
        let Some(data) = hex_board(hex, width, height) else {
            eprintln!("bad board hex on level {level_number}");
            return ExitCode::FAILURE;
        };
        let board = match Board::from_bytes(width, height, &data) {
            Ok(board) => board,
            Err(err) => {
                eprintln!("board: {err}");
                return ExitCode::FAILURE;
            }
        };
        helper.reset();
        let (moves, has_corral) = match find_possible_moves(
            &ctx,
            &board,
            false,
            SearchMode::ForwardWithBases,
            &mut helper,
            &mut state,
        ) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("moves: {err}");
                return ExitCode::FAILURE;
            }
        };
        out.push_str(&format!(
            "{hash} {} {}\n",
            u8::from(has_corral),
            moves.len()
        ));
        for mv in &moves {
            out.push_str(&format!(
                "  {} {} {} {} {} {} {} {} {}\n",
                mv.from,
                mv.to,
                mv.sokoban_position,
                u8::from(mv.kill),
                u8::from(mv.base),
                u8::from(mv.pull),
                mv.attr.weight,
                u8::from(mv.attr.advisor),
                u8::from(mv.attr.reversible),
            ));
        }
    }
    if let Err(err) = fs::write(out_path, out) {
        eprintln!("{}: {err}", out_path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Mirrors `refcheck fwb`: FORWARD_WITH_BASES scores after planning.
fn fwb_score(
    levels_path: &Path,
    level_number: usize,
    plan_path: &Path,
    boards_path: &Path,
    out_path: &Path,
) -> ExitCode {
    use festival_core::constants::SearchMode;
    use festival_core::planning::{oop, park_order};
    use festival_core::search::score::score_board;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let plan = match load_plan(plan_path) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let mut helper = Helper {
        parking_order: plan,
        ..Helper::default()
    };
    if let Err(err) = park_order::verify_parking_order(&ctx, &helper) {
        eprintln!("plan: {err}");
        return ExitCode::FAILURE;
    }
    if let Err(err) = park_order::reduce_parking_order(&ctx, &mut helper) {
        eprintln!("plan: {err}");
        return ExitCode::FAILURE;
    }
    if let Err(err) = oop::prepare_oop_zones(&ctx, &mut helper) {
        eprintln!("oop: {err}");
        return ExitCode::FAILURE;
    }
    helper.init_scored_distance(&ctx, &ctx.level.board, false);

    let boards_text = match fs::read_to_string(boards_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{}: {err}", boards_path.display());
            return ExitCode::FAILURE;
        }
    };
    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let mut out = String::new();
    for line in boards_text.lines() {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else {
            continue;
        };
        let Some(hex) = fields.next() else {
            continue;
        };
        let Some(data) = hex_board(hex, width, height) else {
            eprintln!("bad board hex on level {level_number}");
            return ExitCode::FAILURE;
        };
        let board = match Board::from_bytes(width, height, &data) {
            Ok(board) => board,
            Err(err) => {
                eprintln!("board: {err}");
                return ExitCode::FAILURE;
            }
        };
        let score = score_board(&ctx, &helper, &board, false, SearchMode::ForwardWithBases);
        out.push_str(&format!(
            "{hash} {} {} {} {} {} {} {}\n",
            score.boxes_in_level,
            score.packed_boxes,
            score.connectivity,
            score.rooms_score,
            score.out_of_plan,
            score.hotspots,
            score.dist_to_targets,
        ));
    }
    if let Err(err) = fs::write(out_path, out) {
        eprintln!("{}: {err}", out_path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn load_plan(path: &Path) -> Result<Vec<festival_core::planning::ParkOrder>, String> {
    let text = fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let mut plan = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(first) = fields.next() else {
            continue;
        };
        let Ok(from) = first.parse::<i32>() else {
            continue; // "expansions N" / "plan N"
        };
        let Some(to) = fields.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(until) = fields.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        plan.push(festival_core::planning::ParkOrder { from, to, until });
    }
    Ok(plan)
}

/// GIRL score/value trace on a fixed board sample and parking plan; the C++
/// counterpart is `refcheck girl`.
fn girl_trace(
    levels_path: &Path,
    level_number: usize,
    plan_path: &Path,
    boards_path: &Path,
    out_path: &Path,
) -> ExitCode {
    use festival_core::constants::SearchMode;
    use festival_core::planning::park_order;
    use festival_core::search::girl::score_to_value;
    use festival_core::search::score::score_board;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(3600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let plan = match load_plan(plan_path) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let mut helper = Helper {
        parking_order: plan,
        ..Helper::default()
    };
    if let Err(err) = park_order::verify_parking_order(&ctx, &helper) {
        eprintln!("plan: {err}");
        return ExitCode::FAILURE;
    }
    if let Err(err) = park_order::reduce_parking_order(&ctx, &mut helper) {
        eprintln!("plan: {err}");
        return ExitCode::FAILURE;
    }

    let boards_text = match fs::read_to_string(boards_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{}: {err}", boards_path.display());
            return ExitCode::FAILURE;
        }
    };

    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let mut out = String::new();

    for line in boards_text.lines() {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else {
            continue;
        };
        let Some(hex) = fields.next() else {
            continue;
        };
        let Some(data) = hex_board(hex, width, height) else {
            eprintln!("bad board hex on level {level_number}");
            return ExitCode::FAILURE;
        };
        let board = match Board::from_bytes(width, height, &data) {
            Ok(board) => board,
            Err(err) => {
                eprintln!("board: {err}");
                return ExitCode::FAILURE;
            }
        };

        let push = score_board(&ctx, &helper, &board, false, SearchMode::GirlSearch);
        let pull = score_board(&ctx, &helper, &board, true, SearchMode::GirlSearch);
        out.push_str(&format!(
            "{hash} {} {} {} {} {} {:.17e} {} {} {} {:.17e}\n",
            push.packed_boxes,
            push.boxes_on_targets,
            push.connectivity,
            push.dist_to_targets,
            push.overlap,
            score_to_value(&push, false, &ctx.girl),
            pull.boxes_on_targets,
            pull.connectivity,
            pull.dist_to_targets,
            score_to_value(&pull, true, &ctx.girl),
        ));
    }

    if let Err(err) = fs::write(out_path, out) {
        eprintln!("{}: {err}", out_path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn packing_plan(
    levels_path: &Path,
    level_number: usize,
    strategy: usize,
    log_nodes: u32,
) -> ExitCode {
    use festival_core::search::packing::{PackingConfig, run_packing};
    use festival_core::search::perimeter::Perimeter;
    use festival_core::search::tree::Tree;
    use festival_core::solver::strategy::BACKWARD_SEARCH_TYPES;

    let raws = match load_levels(levels_path) {
        Ok(raws) => raws,
        Err(err) => {
            eprintln!("{}: {err}", levels_path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(raw) = raws.into_iter().nth(level_number.saturating_sub(1)) else {
        eprintln!("no such level: {level_number}");
        return ExitCode::FAILURE;
    };
    let Some(&mode) = BACKWARD_SEARCH_TYPES.get(strategy) else {
        eprintln!("strategy must be 0..7");
        return ExitCode::FAILURE;
    };
    let level = match Level::preprocess(raw) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("level {level_number}: {}", err.reason());
            return ExitCode::FAILURE;
        }
    };
    let clock = SearchClock::new(Some(Duration::from_secs(3600)), None);
    let ctx = match LevelContext::analyse(level, &clock) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut tree = match Tree::new_with_level(log_nodes, &ctx.level) {
        Ok(tree) => tree,
        Err(err) => {
            eprintln!("tree: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = tree.prepare_deadlock(&ctx) {
        eprintln!("deadlock: {err}");
        return ExitCode::FAILURE;
    }
    let mut helper = Helper::default();
    let perimeter = Perimeter::new(log_nodes.min(22));
    helper.weighted_search = true;
    let run = match run_packing(
        &ctx,
        &mut tree,
        &mut helper,
        &perimeter,
        &PackingConfig {
            time_limit: Some(Duration::from_secs(3600)),
            max_expansions: None,
            cancel: None,
        },
        mode,
    ) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("level {level_number}: {err}");
            return ExitCode::FAILURE;
        }
    };

    println!("expansions {}", run.expansions);
    println!("plan {}", helper.parking_order.len());
    for step in &helper.parking_order {
        println!("{} {} {}", step.from, step.to, step.until);
    }
    ExitCode::SUCCESS
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

    let titles: HashMap<&str, usize> = raws
        .iter()
        .enumerate()
        .filter(|(_, raw)| !raw.title.is_empty() && raw.title != "None")
        .rev()
        .map(|(index, raw)| (raw.title.as_str(), index + 1))
        .collect();

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
            if let Some(&number) = titles.get(line.trim()) {
                current_level = Some(number);
            }
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
