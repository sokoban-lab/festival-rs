// Phase 2 end-to-end: forward FESS must produce legal LURD solutions.

use std::path::PathBuf;
use std::time::Duration;

use festival_core::io::lurd;
use festival_core::io::sok::load_levels;
use festival_core::search::FessConfig;
use festival_core::solver::{StrategyConfig, solve_forward, solve_strategy};
use festival_core::{Level, LevelContext, SearchClock};

fn smoke_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../levels/smoke/smoke.sok")
}

#[test]
fn forward_search_solves_smoke_levels() {
    let raws = load_levels(&smoke_path()).expect("smoke levels");
    let mut solved = 0usize;

    for (index, raw) in raws.into_iter().enumerate() {
        let title = raw.title.clone();
        let level_number = index + 1;
        let level = Level::preprocess(raw)
            .unwrap_or_else(|err| panic!("level {level_number}: {}", err.reason()));

        let analysis_clock = SearchClock::new(Some(Duration::from_secs(60)), None);
        let ctx = LevelContext::analyse(level, &analysis_clock).unwrap();

        let config = FessConfig {
            time_limit: Some(Duration::from_secs(20)),
            max_expansions: Some(500_000),
            log_max_nodes: 18,
            save_best: false,
            cancel: None,
        };
        let outcome = solve_forward(&ctx, &config)
            .unwrap_or_else(|err| panic!("level {level_number} ({title}): {err}"));

        if outcome.solved {
            solved += 1;
            let stats = lurd::replay(&ctx.level.initial, &outcome.lurd).unwrap_or_else(|err| {
                panic!("level {level_number} ({title}) produced invalid LURD: {err}")
            });
            assert!(
                stats.solved,
                "level {level_number} ({title}) LURD does not solve the level"
            );
        } else {
            eprintln!(
                "level {level_number} ({title}) not solved: {:?} after {} expansions",
                outcome.failure_reason, outcome.expansions
            );
        }
    }

    assert!(
        solved >= 6,
        "Phase 2 gate requires at least 6/12 smoke levels, solved {solved}"
    );
}

#[test]
fn girl_and_dragonfly_strategies_solve_smoke_levels() {
    let raws = load_levels(&smoke_path()).expect("smoke levels");

    for raw in raws {
        let title = raw.title.clone();
        let level =
            Level::preprocess(raw).unwrap_or_else(|err| panic!("level {title}: {}", err.reason()));
        let clock = SearchClock::new(Some(Duration::from_secs(60)), None);
        let ctx = LevelContext::analyse(level, &clock).unwrap();

        for strategy in [2usize, 7] {
            let outcome = solve_strategy(
                &ctx,
                strategy,
                &StrategyConfig {
                    time_limit: Duration::from_secs(10),
                    log_max_nodes: 18,
                    ..StrategyConfig::default()
                },
            )
            .unwrap_or_else(|err| panic!("{title}, strategy {strategy}: {err}"));
            assert!(
                outcome.solved,
                "{title}, strategy {strategy} was not solved: {:?}",
                outcome.failure_reason
            );
            let stats = lurd::replay(&ctx.level.initial, &outcome.lurd)
                .unwrap_or_else(|err| panic!("{title}, strategy {strategy}: invalid LURD: {err}"));
            assert!(stats.solved, "{title}, strategy {strategy}: LURD failed");
        }
    }
}

#[test]
fn forward_search_is_deterministic_with_an_expansion_budget() {
    let raws = load_levels(&smoke_path()).expect("smoke levels");
    let raw = raws.into_iter().nth(7).expect("level 8");
    let level = Level::preprocess(raw).expect("level");
    let clock = SearchClock::new(None, None);
    let ctx = LevelContext::analyse(level, &clock).unwrap();

    let config = FessConfig {
        time_limit: None,
        max_expansions: Some(500_000),
        log_max_nodes: 18,
        save_best: false,
        cancel: None,
    };
    let first = solve_forward(&ctx, &config).unwrap();
    let second = solve_forward(&ctx, &config).unwrap();
    assert!(first.solved);
    assert_eq!(first.lurd, second.lurd);
    assert_eq!(first.moves, second.moves);
    assert_eq!(first.pushes, second.pushes);
}
