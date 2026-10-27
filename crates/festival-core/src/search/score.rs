// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Score payload and comparison (`score.h` / `score.cpp`).
//!
//! Phase 2 implements the `NORMAL` / `FORWARD_WITH_BASES` family. Planning
//! features (`packed_boxes`, `out_of_plan`, `imagine`, `overlap`) stay at zero
//! until Phase 4; their score fields exist so later phases can fill them
//! without changing the tree layout.

use crate::board::Board;
use crate::constants::SearchMode;
use crate::solver::LevelContext;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScoreElement {
    pub connectivity: i16,
    pub rooms_score: i16,
    pub packed_boxes: i16,
    pub boxes_on_targets: i16,
    pub boxes_in_level: i16,
    pub dist_to_targets: i32,
    pub dist_to_imagined: i32,
    pub hotspots: i16,
    pub out_of_plan: i16,
    pub overlap: i16,
    pub imagine: i16,
    pub value: f32,
    pub biconnect: i16,
}

/// Port of `is_better_score` for the modes implemented so far.
///
/// Other search modes are not reachable from the Phase 2 solver; callers must
/// pass `NORMAL` (or the two base modes, which share its comparison order).
pub fn is_better_score(
    new_score: &ScoreElement,
    old_score: &ScoreElement,
    pull_mode: bool,
    _search_mode: SearchMode,
) -> bool {
    if new_score.boxes_in_level < old_score.boxes_in_level {
        return true;
    }
    if new_score.boxes_in_level > old_score.boxes_in_level {
        return false;
    }

    if !pull_mode {
        if new_score.packed_boxes > old_score.packed_boxes {
            return true;
        }
        if new_score.packed_boxes < old_score.packed_boxes {
            return false;
        }

        if new_score.connectivity < old_score.connectivity {
            return true;
        }
        if new_score.connectivity > old_score.connectivity {
            return false;
        }

        if new_score.rooms_score < old_score.rooms_score {
            return true;
        }
        if new_score.rooms_score > old_score.rooms_score {
            return false;
        }

        if new_score.out_of_plan < old_score.out_of_plan {
            return true;
        }
        if new_score.out_of_plan > old_score.out_of_plan {
            return false;
        }

        if new_score.hotspots < old_score.hotspots {
            return true;
        }
        if new_score.hotspots > old_score.hotspots {
            return false;
        }

        if new_score.dist_to_targets > old_score.dist_to_targets {
            return true;
        }
        if new_score.dist_to_targets < old_score.dist_to_targets {
            return false;
        }

        return false;
    }

    if new_score.connectivity < old_score.connectivity {
        return true;
    }
    if new_score.connectivity > old_score.connectivity {
        return false;
    }

    if new_score.boxes_on_targets < old_score.boxes_on_targets {
        return true;
    }
    if new_score.boxes_on_targets > old_score.boxes_on_targets {
        return false;
    }

    if new_score.rooms_score < old_score.rooms_score {
        return true;
    }
    if new_score.rooms_score > old_score.rooms_score {
        return false;
    }

    if new_score.dist_to_targets > old_score.dist_to_targets {
        return true;
    }
    if new_score.dist_to_targets < old_score.dist_to_targets {
        return false;
    }

    false
}

/// Sum of the per-cell push distance for every box (`get_scored_distance`).
pub fn get_scored_distance(ctx: &LevelContext, board: &Board) -> i32 {
    let mut sum = 0i32;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & crate::constants::BOX == 0 {
                continue;
            }
            sum += ctx.scored_distance[Board::flat_index(y, x)] as i32;
        }
    }
    sum
}

/// Port of `score_board` restricted to `NORMAL` / base modes.
pub fn score_board(
    ctx: &LevelContext,
    board: &Board,
    pull_mode: bool,
    search_mode: SearchMode,
) -> ScoreElement {
    let level = &ctx.level;
    let mut score = ScoreElement {
        boxes_on_targets: board.boxes_on_targets() as i16,
        connectivity: crate::bfs::get_connectivity(level, board) as i16,
        boxes_in_level: board.boxes_in_level() as i16,
        ..ScoreElement::default()
    };

    match search_mode {
        SearchMode::Normal | SearchMode::BaseSearch | SearchMode::ForwardWithBases => {}
        SearchMode::DeadlockSearch => {
            score.hotspots = crate::deadlock::zone::size_of_deadlock_zone(board) as i16;
            return score;
        }
        SearchMode::KDistSearch | SearchMode::XySearch | SearchMode::MiniSearch => {
            return score;
        }
        SearchMode::WobblersSearch | SearchMode::RoomsDeadlockSearch => {
            score.boxes_on_targets = if pull_mode {
                board.boxes_on_bases(&level.initial) as i16
            } else {
                board.boxes_on_targets() as i16
            };
            return score;
        }
        // Feature-specific scoring is Phase 4/5. Keep a deterministic base
        // score instead of inventing semantics.
        _ => return score,
    }

    score.rooms_score = ctx.rooms.score(level, board) as i16;
    if pull_mode {
        // Pull-mode scored distance needs the reverse helper matrix; the
        // forward Phase 2 search does not use it.
        score.dist_to_targets = 0;
        return score;
    }

    score.dist_to_targets = get_scored_distance(ctx, board);
    score.packed_boxes = 0;
    score.hotspots = ctx.hotspots.score(level, board) as i16;
    score.out_of_plan = 0;
    score.imagine = 0;
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;
    use std::time::Duration;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = crate::solver::SearchClock::new(Some(Duration::from_secs(600)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    fn score(packed: i16, connectivity: i16, dist: i32) -> ScoreElement {
        ScoreElement {
            packed_boxes: packed,
            connectivity,
            dist_to_targets: dist,
            ..ScoreElement::default()
        }
    }

    #[test]
    fn normal_comparison_prefers_packed_then_connectivity() {
        assert!(is_better_score(
            &score(2, 5, 0),
            &score(1, 1, 100),
            false,
            SearchMode::Normal
        ));
        assert!(is_better_score(
            &score(1, 1, 0),
            &score(1, 2, 0),
            false,
            SearchMode::Normal
        ));
        assert!(is_better_score(
            &score(1, 1, 5),
            &score(1, 1, 4),
            false,
            SearchMode::Normal
        ));
    }

    #[test]
    fn push_scored_distance_counts_pushes_to_targets() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        // One box two pushes away from the target.
        assert_eq!(get_scored_distance(&ctx, &ctx.level.board), 2);
    }

    #[test]
    fn board_score_tracks_packed_boxes() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let score = score_board(&ctx, &ctx.level.board, false, SearchMode::Normal);
        assert_eq!(score.boxes_in_level, 1);
        assert_eq!(score.boxes_on_targets, 0);
        assert!(score.connectivity >= 2);
    }
}
