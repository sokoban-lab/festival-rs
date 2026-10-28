// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Score payload and comparison (`score.h` / `score.cpp`).
//!
//! Phase 4 adds the HF / BICON / MAX_DIST / REV families and the planning
//! features (`packed_boxes`, `out_of_plan`, `imagine`, `dist_to_imagined`).
//! Phase 6 adds the SNAIL / NETLOCK payloads.

use crate::board::Board;
use crate::constants::SearchMode;
use crate::planning::{imagine, oop, park_order};
use crate::search::girl;
use crate::search::helper::Helper;
use crate::search::hf;
use crate::search::snail;
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
    /// Back-propagated GIRL value (`girl.cpp` keeps this in `f32`).
    pub value: f32,
    /// GIRL `score_to_value` of the raw features. Kept separate because the
    /// back-propagated `value` must not leak into `is_better_score`.
    pub girl_value: f64,
    pub biconnect: i16,
}

/// Port of `is_better_score`.
pub fn is_better_score(
    new_score: &ScoreElement,
    old_score: &ScoreElement,
    pull_mode: bool,
    search_mode: SearchMode,
) -> bool {
    if matches!(
        search_mode,
        SearchMode::HfSearch
            | SearchMode::BiconSearch
            | SearchMode::MaxDistSearch
            | SearchMode::MaxDistSearch2
    ) {
        return hf::is_better_hf_score(new_score, old_score, pull_mode);
    }

    if search_mode == SearchMode::NaiveSearch {
        if new_score.dist_to_targets < old_score.dist_to_targets {
            return true;
        }
        if new_score.dist_to_targets > old_score.dist_to_targets {
            return false;
        }
        return new_score.packed_boxes > old_score.packed_boxes;
    }

    if search_mode == SearchMode::RevSearch {
        return hf::is_better_rev_score(new_score, old_score, pull_mode);
    }

    if search_mode == SearchMode::SnailSearch {
        return snail::is_better_snail_score(new_score, old_score, pull_mode);
    }

    if search_mode == SearchMode::NetlockSearch {
        return snail::is_better_netlock_score(new_score, old_score);
    }

    if search_mode == SearchMode::GirlSearch {
        // The stored raw-feature value; see `ScoreElement::girl_value`.
        return new_score.girl_value > old_score.girl_value;
    }

    // The k-distance deadlock search compares positions directly.
    if search_mode == SearchMode::KDistSearch {
        return false;
    }

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

/// Sum of the per-cell scored distance for every box (`get_scored_distance`).
pub fn get_scored_distance(helper: &Helper, board: &Board) -> i32 {
    let mut sum = 0i32;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & crate::constants::BOX == 0 {
                continue;
            }
            sum += helper.scored_distance[Board::flat_index(y, x)] as i32;
        }
    }
    sum
}

/// Port of `score_board`.
pub fn score_board(
    ctx: &LevelContext,
    helper: &Helper,
    board: &Board,
    pull_mode: bool,
    search_mode: SearchMode,
) -> ScoreElement {
    let level = &ctx.level;
    let mut score = ScoreElement {
        boxes_in_level: level.box_count as i16,
        ..ScoreElement::default()
    };

    if search_mode == SearchMode::NaiveSearch {
        score.packed_boxes = crate::search::naive::fill_naive_scores(&level.initial, board);
        return score;
    }

    if search_mode == SearchMode::SnailSearch {
        snail::fill_snail_scores(&ctx.snail, level, board, &mut score, helper.alg_type);
        return score;
    }

    if search_mode == SearchMode::NetlockSearch {
        snail::fill_netlock_scores(&ctx.snail, level, board, &mut score, helper.alg_type);
        return score;
    }

    score.boxes_on_targets = board.boxes_on_targets() as i16;
    score.connectivity = crate::bfs::get_connectivity(level, board) as i16;
    score.boxes_in_level = board.boxes_in_level() as i16;

    match search_mode {
        SearchMode::KDistSearch | SearchMode::XySearch | SearchMode::MiniSearch => {
            return score;
        }
        SearchMode::DeadlockSearch => {
            score.hotspots = crate::deadlock::zone::size_of_deadlock_zone(board) as i16;
            return score;
        }
        SearchMode::RevSearch => {
            hf::set_score_in_rev_mode(ctx, helper, board, &mut score, pull_mode);
            return score;
        }
        SearchMode::HfSearch
        | SearchMode::BiconSearch
        | SearchMode::MaxDistSearch
        | SearchMode::MaxDistSearch2 => {
            hf::fill_hf_scores(ctx, helper, board, &mut score, pull_mode, search_mode);
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
        SearchMode::GirlSearch => {
            girl::fill_girl_scores(ctx, helper, board, &mut score, pull_mode);
            score.girl_value = girl::score_to_value(&score, pull_mode, &ctx.girl);
            return score;
        }
        SearchMode::Normal | SearchMode::BaseSearch | SearchMode::ForwardWithBases => {}
        _ => return score,
    }

    score.rooms_score = ctx.rooms.score(level, board) as i16;
    score.dist_to_targets = get_scored_distance(helper, board);

    if pull_mode {
        return score;
    }

    score.packed_boxes = park_order::score_parked_boxes(ctx, board, helper) as i16;
    score.hotspots = ctx.hotspots.score(level, board) as i16;
    score.out_of_plan = oop::get_out_of_plan_score(board, score.packed_boxes as i32, helper) as i16;
    score.imagine = imagine::imagine_score(ctx, board, search_mode, helper) as i16;
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
        let mut helper = Helper::default();
        helper.init_scored_distance(&ctx, &ctx.level.board, false);
        // One box two pushes away from the target.
        assert_eq!(get_scored_distance(&helper, &ctx.level.board), 2);
    }

    #[test]
    fn pull_scored_distance_sums_push_distance_to_targets() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let mut helper = Helper::default();
        helper.init_scored_distance(&ctx, &ctx.level.board, true);
        assert_eq!(get_scored_distance(&helper, &ctx.level.board), 2);
    }

    #[test]
    fn board_score_tracks_packed_boxes() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let helper = Helper::default();
        let score = score_board(&ctx, &helper, &ctx.level.board, false, SearchMode::Normal);
        assert_eq!(score.boxes_in_level, 1);
        assert_eq!(score.boxes_on_targets, 0);
        assert!(score.connectivity >= 2);
    }
}
