#![no_main]

use std::sync::OnceLock;
use std::time::Duration;

use festival_core::deadlock::state::DeadlockState;
use festival_core::moves::find_possible_moves;
use festival_core::search::helper::Helper;
use festival_core::{Level, LevelContext, SearchClock, SearchMode};
use libfuzzer_sys::fuzz_target;

const MODES: [SearchMode; 6] = [
    SearchMode::Normal,
    SearchMode::ForwardWithBases,
    SearchMode::HfSearch,
    SearchMode::BiconSearch,
    SearchMode::MaxDistSearch,
    SearchMode::RevSearch,
];

static CTX: OnceLock<LevelContext> = OnceLock::new();

fn ctx() -> &'static LevelContext {
    CTX.get_or_init(|| {
        let raw = festival_core::io::sok::parse_levels(
            "#########\n#@ $ .  #\n#  $ .  #\n#########\n",
        )
        .remove(0);
        let level = Level::preprocess(raw).expect("fixture level");
        LevelContext::analyse(
            level,
            &SearchClock::new(Some(Duration::from_secs(10)), None),
        )
        .expect("fixture analysis")
    })
}

fuzz_target!(|data: &[u8]| {
    let ctx = ctx();
    let mode = MODES[data.first().copied().unwrap_or(0) as usize % MODES.len()];
    let pull = data.get(1).is_some_and(|byte| byte & 1 == 1);
    let Ok(mut state) = DeadlockState::prepare(ctx) else {
        return;
    };
    let mut helper = Helper::default();
    let _ = find_possible_moves(ctx, &ctx.level.board, pull, mode, &mut helper, &mut state);
});
