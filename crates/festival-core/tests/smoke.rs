// Smoke integration test: every bundled level goes through parse ->
// preprocess -> static analysis, and the first level's known solution replays.

use std::path::PathBuf;
use std::time::Duration;

use festival_core::io::lurd;
use festival_core::io::sok::load_levels;
use festival_core::{Level, LevelContext, SearchClock};

fn smoke_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../levels/smoke/smoke.sok")
}

#[test]
fn all_smoke_levels_analyse() {
    let raws = load_levels(&smoke_path()).expect("smoke levels");
    assert_eq!(raws.len(), 12, "smoke set must contain 12 levels");

    for (index, raw) in raws.into_iter().enumerate() {
        let title = raw.title.clone();
        let level = Level::preprocess(raw)
            .unwrap_or_else(|err| panic!("level {} ({}): {}", index + 1, title, err.reason()));
        assert!(
            level.box_count > 0,
            "level {} ({}) lost all boxes during preprocessing",
            index + 1,
            title
        );
        assert!(level.index_num() > 0);

        let clock = SearchClock::new(Some(Duration::from_secs(600)), None);
        let context = LevelContext::analyse(level, &clock)
            .unwrap_or_else(|err| panic!("level {} ({}): {}", index + 1, title, err.reason()));
        assert!(!context.hotspots.aborted);
    }
}

#[test]
fn first_smoke_level_solution_replays() {
    let raws = load_levels(&smoke_path()).expect("smoke levels");
    let level = Level::preprocess(raws.into_iter().next().unwrap()).expect("level 1");
    let stats = lurd::validate(&level.initial, "L").expect("legal solution");
    assert!(stats.solved);
    assert_eq!(stats.pushes, 1);
}
