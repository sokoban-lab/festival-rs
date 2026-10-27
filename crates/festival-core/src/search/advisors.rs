// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Advisor selection (`advisors.cpp`).
//!
//! Phase 2 implements the advisors that only need the four basic score fields.
//! Planning advisors (opener, explorer, out-of-plan, imagine) stay at `-1`
//! until Phase 4.

use crate::constants::SearchMode;
use crate::moves::Move;
use crate::search::score::{ScoreElement, is_better_score};

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
    OutOfPlan,
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
            Criterion::OutOfPlan => score.out_of_plan < base_score.out_of_plan,
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

/// Port of `set_advisors_inner` restricted to Phase 2 score fields.
pub fn set_advisors_inner(
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    pull_mode: bool,
    search_mode: SearchMode,
    already_expanded: &[bool],
) -> AdvisorsElement {
    let mut advisors = AdvisorsElement::default();

    if matches!(
        search_mode,
        SearchMode::GirlSearch | SearchMode::NaiveSearch | SearchMode::SnailSearch
    ) {
        return advisors;
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
            base_score,
            moves,
            scores,
            pull_mode,
            search_mode,
            already_expanded,
        );
        return advisors;
    }

    if pull_mode {
        // Packing / room / explorer advisors depend on Phase 4 planning data.
        return advisors;
    }

    advisors.pack = choose_advisor(base_score, scores, Criterion::Packed, false);
    advisors.connectivity = choose_advisor(base_score, scores, Criterion::Connectivity, false);
    advisors.rooms = choose_advisor(base_score, scores, Criterion::Rooms, false);
    advisors.hotspots = choose_advisor(base_score, scores, Criterion::Hotspots, false);
    advisors.out_of_plan = choose_advisor(base_score, scores, Criterion::OutOfPlan, false);

    advisors
}

fn default_advisor(
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
        // `from` is a box cell; pulling from a hole is checked by the caller in
        // C++, which has the target-hole mask. Phase 2 leaves it to Phase 3.
        let _ = moves.get(index);
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
