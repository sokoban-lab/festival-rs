//! Gate G2 corpus regression (Microban I).
//!
//! The fast test parses and preprocesses the vendored corpus. The long tests
//! reproduce the two Gate G2 measurements; run them explicitly with
//! `cargo test -p festival-core --test corpus -- --ignored`.

use std::path::PathBuf;
use std::time::Duration;

use festival_core::io::lurd;
use festival_core::io::sok::load_levels;
use festival_core::search::FessConfig;
use festival_core::solver::solve_forward;
use festival_core::{Level, LevelContext, SearchClock};

const LEVELS: usize = 155;

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../levels/corpus/microban.xsb")
}

fn config(time_limit: Duration) -> FessConfig {
    FessConfig {
        time_limit: Some(time_limit),
        max_expansions: None,
        log_max_nodes: 20,
        save_best: false,
        cancel: None,
    }
}

#[test]
fn microban_parses_and_preprocesses() {
    let raws = load_levels(&corpus_path()).expect("Microban corpus");
    assert_eq!(raws.len(), LEVELS, "unexpected Microban level count");

    let mut failures = Vec::new();
    for (index, raw) in raws.into_iter().enumerate() {
        if let Err(err) = Level::preprocess(raw) {
            failures.push((index + 1, err.reason()));
        }
    }

    assert!(
        failures.is_empty(),
        "{} levels failed preprocessing: {failures:?}",
        failures.len()
    );
}

/// Gate G2: Microban 1-40 with `-alg 0 -cores 1 -time 60` must solve at least
/// 30 levels, and every produced solution must pass the independent LURD
/// replay validator.
#[test]
#[ignore = "Gate G2 benchmark: up to 40 x 60s"]
fn microban_first_40_solve_rate() {
    let raws = load_levels(&corpus_path()).expect("Microban corpus");
    let limit = Duration::from_secs(60);
    let mut solved = 0usize;
    let mut invalid = Vec::new();

    for (index, raw) in raws.into_iter().take(40).enumerate() {
        let level_number = index + 1;
        let level = Level::preprocess(raw)
            .unwrap_or_else(|err| panic!("level {level_number}: {}", err.reason()));
        let analysis_clock = SearchClock::new(Some(limit), None);
        let ctx = LevelContext::analyse(level, &analysis_clock)
            .unwrap_or_else(|err| panic!("level {level_number}: {err}"));

        match solve_forward(&ctx, &config(limit)) {
            Ok(outcome) if outcome.solved => {
                solved += 1;
                let stats = lurd::replay(&ctx.level.initial, &outcome.lurd).unwrap_or_else(|err| {
                    panic!("level {level_number} produced invalid LURD: {err}")
                });
                if !stats.solved {
                    invalid.push(level_number);
                }
            }
            Ok(_) => {}
            Err(err) => panic!("level {level_number}: {err}"),
        }
    }

    eprintln!("solved {solved}/40");
    assert!(invalid.is_empty(), "invalid solutions: {invalid:?}");
    assert!(
        solved >= 30,
        "Gate G2 requires at least 30/40 solved, got {solved}/40"
    );
}

/// Gate G2: the first 100 corpus levels must run without panics or bounds
/// errors, and any solution produced must replay correctly.
#[test]
#[ignore = "Gate G2 benchmark: 100-level sweep"]
fn microban_first_100_no_panic() {
    let raws = load_levels(&corpus_path()).expect("Microban corpus");
    let limit = Duration::from_millis(250);
    let mut solved = 0usize;

    for (index, raw) in raws.into_iter().take(100).enumerate() {
        let level_number = index + 1;
        let level = Level::preprocess(raw)
            .unwrap_or_else(|err| panic!("level {level_number}: {}", err.reason()));
        let analysis_clock = SearchClock::new(Some(limit), None);
        let ctx = LevelContext::analyse(level, &analysis_clock)
            .unwrap_or_else(|err| panic!("level {level_number}: {err}"));

        let outcome = solve_forward(&ctx, &config(limit))
            .unwrap_or_else(|err| panic!("level {level_number}: {err}"));
        if outcome.solved {
            solved += 1;
            let stats = lurd::replay(&ctx.level.initial, &outcome.lurd)
                .unwrap_or_else(|err| panic!("level {level_number} produced invalid LURD: {err}"));
            assert!(
                stats.solved,
                "level {level_number}: LURD does not solve the level"
            );
        }
    }

    eprintln!("completed 100 levels, solved {solved}/100");
}
