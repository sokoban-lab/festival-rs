// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Advisor selection (`advisors.cpp`).

use crate::board::Board;
use crate::constants::SearchMode;
use crate::deadlock::state::DeadlockState;
use crate::error::Result;
use crate::moves::Move;
use crate::planning::{heuristics, imagine, oop, opener};
use crate::search::helper::Helper;
use crate::search::hf;
use crate::search::score::{ScoreElement, is_better_score};
use crate::solver::LevelContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvisorsElement {
    pub pack: i32,
    pub connectivity: i32,
    pub rooms: i32,
    pub hotspots: i32,
    pub opener: i32,
    pub out_of_plan: i32,
    pub explorer: i32,
    pub imagine: i32,
}

impl Default for AdvisorsElement {
    fn default() -> Self {
        Self {
            pack: -1,
            connectivity: -1,
            rooms: -1,
            hotspots: -1,
            opener: -1,
            out_of_plan: -1,
            explorer: -1,
            imagine: -1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Criterion {
    Packed,
    Connectivity,
    Rooms,
    Hotspots,
}

fn choose_advisor(
    base_score: &ScoreElement,
    scores: &[ScoreElement],
    criterion: Criterion,
    pull_mode: bool,
) -> i32 {
    let mut best_move = -1i32;
    let mut best_score = ScoreElement::default();

    for (index, score) in scores.iter().enumerate() {
        if score.packed_boxes < base_score.packed_boxes {
            continue;
        }

        let better = match criterion {
            Criterion::Packed => score.packed_boxes > base_score.packed_boxes,
            Criterion::Connectivity => score.connectivity < base_score.connectivity,
            Criterion::Rooms => score.rooms_score < base_score.rooms_score,
            Criterion::Hotspots => score.hotspots < base_score.hotspots,
        };
        if !better {
            continue;
        }

        if best_move == -1 {
            best_move = index as i32;
            best_score = *score;
            continue;
        }

        if is_better_score(score, &best_score, pull_mode, SearchMode::Normal) {
            best_score = *score;
            best_move = index as i32;
        }
    }

    best_move
}

/// Port of `set_default_advisor` (HF family).
fn default_advisor(
    ctx: &LevelContext,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    pull_mode: bool,
    search_mode: SearchMode,
    already_expanded: &[bool],
) -> i32 {
    let mut best = *base_score;
    let mut result = -1i32;

    for (index, score) in scores.iter().enumerate() {
        if pull_mode {
            let (y, x) = ctx.level.coord(moves[index].from as usize);
            if ctx.level.target_holes.cell(y, x) != 0 {
                continue;
            }
        }
        if already_expanded.get(index).copied().unwrap_or(false) {
            continue;
        }
        if is_better_score(score, &best, pull_mode, search_mode) {
            best = *score;
            result = index as i32;
        }
    }
    result
}

/// Port of `discard_advisors_in_base_search`.
fn discard_advisors_in_base_search(moves: &[Move], advisors: &mut AdvisorsElement) {
    if advisors.pack == -1 {
        return;
    }
    if moves[advisors.pack as usize].base {
        advisors.connectivity = -1;
        advisors.rooms = -1;
        advisors.hotspots = -1;
        advisors.opener = -1;
        advisors.out_of_plan = -1;
        advisors.explorer = -1;
        advisors.imagine = -1;
    }
}

/// Port of `set_advisors_inner`.
#[allow(clippy::too_many_arguments)]
pub fn set_advisors_inner(
    ctx: &LevelContext,
    helper: &Helper,
    deadlock: &mut DeadlockState,
    board: &Board,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    pull_mode: bool,
    search_mode: SearchMode,
    already_expanded: &[bool],
) -> Result<AdvisorsElement> {
    let mut advisors = AdvisorsElement::default();

    if matches!(
        search_mode,
        SearchMode::GirlSearch | SearchMode::NaiveSearch | SearchMode::SnailSearch
    ) {
        return Ok(advisors);
    }

    if matches!(
        search_mode,
        SearchMode::HfSearch
            | SearchMode::BiconSearch
            | SearchMode::MaxDistSearch
            | SearchMode::MaxDistSearch2
            | SearchMode::NetlockSearch
    ) {
        advisors.pack = default_advisor(
            ctx,
            base_score,
            moves,
            scores,
            pull_mode,
            search_mode,
            already_expanded,
        );
        return Ok(advisors);
    }

    if search_mode == SearchMode::RevSearch {
        advisors.pack = hf::set_rev_advisor(
            ctx,
            board,
            base_score,
            moves,
            scores,
            pull_mode,
            search_mode,
            already_expanded,
        );
        return Ok(advisors);
    }

    if pull_mode {
        advisors.pack = heuristics::packing_search_advisor(
            ctx,
            deadlock,
            board,
            base_score,
            moves,
            scores,
            search_mode,
        )?;
        advisors.connectivity = choose_advisor(base_score, scores, Criterion::Connectivity, true);
        advisors.rooms = choose_advisor(base_score, scores, Criterion::Rooms, true);
        advisors.explorer = opener::explorer(ctx, board, base_score, moves, scores, pull_mode);

        if search_mode == SearchMode::BaseSearch {
            discard_advisors_in_base_search(moves, &mut advisors);
        }
        return Ok(advisors);
    }

    advisors.pack = choose_advisor(base_score, scores, Criterion::Packed, false);
    advisors.connectivity = choose_advisor(base_score, scores, Criterion::Connectivity, false);
    advisors.rooms = choose_advisor(base_score, scores, Criterion::Rooms, false);
    advisors.hotspots = choose_advisor(base_score, scores, Criterion::Hotspots, false);
    advisors.imagine = imagine::choose_imagine_advisor(base_score, scores, search_mode);
    advisors.out_of_plan =
        oop::new_oop_advisor(ctx, board, base_score, moves, scores, pull_mode, helper);
    advisors.opener =
        opener::pick_opener(ctx, board, base_score, moves, scores, pull_mode, helper)?;
    advisors.explorer = opener::explorer(ctx, board, base_score, moves, scores, pull_mode);

    Ok(advisors)
}

pub fn is_an_advisor(advisors: &AdvisorsElement, son: usize) -> bool {
    let son = son as i32;
    advisors.pack == son
        || advisors.connectivity == son
        || advisors.rooms == son
        || advisors.hotspots == son
        || advisors.opener == son
        || advisors.out_of_plan == son
        || advisors.explorer == son
        || advisors.imagine == son
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(connectivity: i16, rooms: i16) -> ScoreElement {
        ScoreElement {
            connectivity,
            rooms_score: rooms,
            ..ScoreElement::default()
        }
    }

    #[test]
    fn connectivity_advisor_prefers_fewer_components() {
        let base = score(3, 0);
        let scores = [score(3, 0), score(2, 0), score(4, 0)];
        assert_eq!(
            choose_advisor(&base, &scores, Criterion::Connectivity, false),
            1
        );
    }

    #[test]
    fn advisors_are_detected_by_index() {
        let advisors = AdvisorsElement {
            connectivity: 2,
            ..AdvisorsElement::default()
        };
        assert!(is_an_advisor(&advisors, 2));
        assert!(!is_an_advisor(&advisors, 3));
    }
}
