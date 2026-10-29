//! Distance scores via minimum-cost assignment (`match_distance.cpp`).
//!
//! A base score is computed once and the children's scores reuse the Hungarian
//! cache: each child only changes the row of the moved box.

use crate::analysis::hungarian::{CostMatrix, HungarianCache, HungarianSolution, solve_hungarian};
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::fixed_boxes::{is_box_2x2_frozen_after_move, is_box_2x2_frozen_at_index};
use crate::distance::Distances;
use crate::level::Level;
use crate::moves::Move;
use crate::search::score::ScoreElement;

fn fill_matrix(
    matrix: &mut CostMatrix,
    boxes: &[usize],
    targets: &[usize],
    distance: impl Fn(usize, usize) -> u32,
) {
    for (i, &from) in boxes.iter().enumerate() {
        for (j, &to) in targets.iter().enumerate() {
            matrix.set(i, j, distance(from, to) as i32);
        }
    }
}

fn row_of_box(boxes: &[usize], from: u16) -> usize {
    boxes
        .iter()
        .position(|&index| index == from as usize)
        .expect("internal error")
}

/// `compute_match_distance`: distance from boxes to the current targets.
pub fn compute_match_distance(
    level: &Level,
    distances: &Distances,
    board: &Board,
    base_score: &mut ScoreElement,
    moves: &[Move],
    scores: &mut [ScoreElement],
) {
    let boxes = level.indices_of_boxes(board);
    let targets = level.indices_of_targets(board);
    debug_assert_eq!(boxes.len(), targets.len(), "box num mismatch1");
    let n = boxes.len();

    let mut matrix = CostMatrix::new(n);
    fill_matrix(&mut matrix, &boxes, &targets, |from, to| {
        distances.from_to(from, to)
    });

    let mut cache = HungarianCache::new();
    let mut solution = HungarianSolution::default();
    solve_hungarian(n, &matrix, &mut solution, &mut cache);
    base_score.dist_to_targets = solution.weight;

    for (t, m) in moves.iter().enumerate() {
        if m.from == m.to {
            scores[t].dist_to_targets = base_score.dist_to_targets;
            continue;
        }

        let mut child_matrix = matrix.clone();
        let row = row_of_box(&boxes, m.from);
        for (j, &target) in targets.iter().enumerate() {
            child_matrix.set(row, j, distances.from_to(m.to as usize, target) as i32);
        }

        solve_hungarian(n, &child_matrix, &mut solution, &mut cache);
        scores[t].dist_to_targets = solution.weight;
    }
}

/// `compute_imagine_distance`: distance from boxes to the imagined (plan) boxes.
///
/// `imagined` is the board produced by `imagine.cpp`; this routine only handles
/// the assignment part.
#[allow(clippy::too_many_arguments)]
pub fn compute_imagine_distance(
    level: &Level,
    distances: &Distances,
    board: &Board,
    imagined: &Board,
    base_score: &mut ScoreElement,
    moves: &[Move],
    scores: &mut [ScoreElement],
    search_mode: SearchMode,
) {
    if matches!(
        search_mode,
        SearchMode::ForwardWithBases | SearchMode::GirlSearch
    ) {
        return;
    }

    let boxes = level.indices_of_boxes(board);
    let targets = level.indices_of_boxes(imagined);
    if boxes.len() != targets.len() {
        // A zero-length reverse phase leaves `imagined_hf_board` unset; C++
        // aborts here, but a degraded score is safer for the scheduler.
        base_score.dist_to_imagined = crate::constants::INF as i32;
        for score in scores.iter_mut() {
            score.dist_to_imagined = crate::constants::INF as i32;
        }
        return;
    }
    let n = boxes.len();

    let mut matrix = CostMatrix::new(n);
    for (i, &from) in boxes.iter().enumerate() {
        for (j, &target) in targets.iter().enumerate() {
            matrix.set(i, j, distances.from_to(from, target) as i32);
        }
        if is_box_2x2_frozen_at_index(level, board, from) {
            for (j, &target) in targets.iter().enumerate() {
                matrix.set(i, j, if target == from { 0 } else { INF as i32 });
            }
        }
    }

    let mut cache = HungarianCache::new();
    let mut solution = HungarianSolution::default();
    solve_hungarian(n, &matrix, &mut solution, &mut cache);
    base_score.dist_to_imagined = solution.weight;

    for (t, m) in moves.iter().enumerate() {
        if m.from == m.to {
            scores[t].dist_to_imagined = base_score.dist_to_imagined;
            continue;
        }

        let mut child_matrix = matrix.clone();
        let row = row_of_box(&boxes, m.from);
        for (j, &target) in targets.iter().enumerate() {
            child_matrix.set(row, j, distances.from_to(m.to as usize, target) as i32);
        }

        if is_box_2x2_frozen_after_move(level, board, m) {
            for (j, &target) in targets.iter().enumerate() {
                child_matrix.set(
                    row,
                    j,
                    if target == m.to as usize {
                        0
                    } else {
                        INF as i32
                    },
                );
            }
        }

        solve_hungarian(n, &child_matrix, &mut solution, &mut cache);
        scores[t].dist_to_imagined = solution.weight;
    }
}

/// `compute_base_distance`: distance from the initial boxes (bases) to the
/// current boxes.
pub fn compute_base_distance(
    level: &Level,
    distances: &Distances,
    board: &Board,
    base_score: &mut ScoreElement,
    moves: &[Move],
    scores: &mut [ScoreElement],
) {
    let boxes = level.indices_of_boxes(board);
    let bases = level.indices_of_initial_bases();
    debug_assert_eq!(boxes.len(), bases.len(), "box num mismatch3");
    let n = boxes.len();

    let mut matrix = CostMatrix::new(n);
    for (i, &box_index) in boxes.iter().enumerate() {
        for (j, &base) in bases.iter().enumerate() {
            // Note the reversed indices: mat[i][j] is base j -> box i.
            matrix.set(i, j, distances.from_to(base, box_index) as i32);
        }
    }

    let mut cache = HungarianCache::new();
    let mut solution = HungarianSolution::default();
    solve_hungarian(n, &matrix, &mut solution, &mut cache);
    base_score.dist_to_targets = solution.weight;

    for (t, m) in moves.iter().enumerate() {
        if m.from == m.to {
            scores[t].dist_to_targets = base_score.dist_to_targets;
            continue;
        }

        let mut child_matrix = matrix.clone();
        let row = row_of_box(&boxes, m.from);
        for (j, &base) in bases.iter().enumerate() {
            child_matrix.set(row, j, distances.from_to(base, m.to as usize) as i32);
        }

        solve_hungarian(n, &child_matrix, &mut solution, &mut cache);
        scores[t].dist_to_targets = solution.weight;
    }
}

/// `compute_rev_distance`: same as the base distance, but with an explicit
/// base position and distance matrix (used after a forward search).
pub fn compute_rev_distance(
    level: &Level,
    board: &Board,
    base_board: &Board,
    from_to: &[u32],
    base_score: &mut ScoreElement,
    moves: &[Move],
    scores: &mut [ScoreElement],
) {
    let n_indices = level.index_num();
    debug_assert!(from_to.len() >= n_indices * n_indices);

    let boxes = level.indices_of_boxes(board);
    let bases = level.indices_of_boxes(base_board);
    if boxes.len() != bases.len() {
        // C++ aborts on this mismatch; it only happens when a degenerate
        // budget skipped `setup_rev_board`. Degrade instead of panicking.
        base_score.dist_to_imagined = crate::constants::INF as i32;
        for score in scores.iter_mut() {
            score.dist_to_imagined = crate::constants::INF as i32;
        }
        return;
    }
    let n = boxes.len();

    let mut matrix = CostMatrix::new(n);
    for (i, &box_index) in boxes.iter().enumerate() {
        for (j, &base) in bases.iter().enumerate() {
            matrix.set(i, j, from_to[base * n_indices + box_index] as i32);
        }
    }

    let mut cache = HungarianCache::new();
    let mut solution = HungarianSolution::default();
    solve_hungarian(n, &matrix, &mut solution, &mut cache);
    base_score.dist_to_imagined = solution.weight;

    for (t, m) in moves.iter().enumerate() {
        if m.from == m.to {
            scores[t].dist_to_imagined = base_score.dist_to_imagined;
            continue;
        }

        let mut child_matrix = matrix.clone();
        let row = row_of_box(&boxes, m.from);
        for (j, &base) in bases.iter().enumerate() {
            child_matrix.set(row, j, from_to[base * n_indices + m.to as usize] as i32);
        }

        solve_hungarian(n, &child_matrix, &mut solution, &mut cache);
        scores[t].dist_to_imagined = solution.weight;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    fn moves(from: u16, to: u16) -> Vec<Move> {
        vec![Move {
            from,
            to,
            ..Default::default()
        }]
    }

    #[test]
    fn matching_distance_tracks_a_push() {
        let lvl = level("#######\n#@ $ .#\n#######\n");
        let distances = crate::distance::compute(&lvl);
        let from = lvl.index_at(1, 3).unwrap() as u16;
        let to = lvl.index_at(1, 4).unwrap() as u16;

        let mut base = ScoreElement::default();
        let mut scores = [ScoreElement::default()];
        compute_match_distance(
            &lvl,
            &distances,
            &lvl.board,
            &mut base,
            &moves(from, to),
            &mut scores,
        );

        assert_eq!(base.dist_to_targets, 2);
        assert_eq!(scores[0].dist_to_targets, 1);
    }

    #[test]
    fn matching_distance_keeps_sentinel_moves() {
        let lvl = level("#######\n#@ $ .#\n#######\n");
        let distances = crate::distance::compute(&lvl);
        let from = lvl.index_at(1, 3).unwrap() as u16;

        let mut base = ScoreElement::default();
        let mut scores = [ScoreElement::default()];
        compute_match_distance(
            &lvl,
            &distances,
            &lvl.board,
            &mut base,
            &moves(from, from),
            &mut scores,
        );
        assert_eq!(scores[0].dist_to_targets, base.dist_to_targets);
    }

    #[test]
    fn base_distance_rewards_boxes_returning_to_bases() {
        let lvl = level("#######\n#@ $ .#\n#######\n");
        let distances = crate::distance::compute(&lvl);
        let base_index = lvl.index_at(1, 3).unwrap() as u16;
        let moved = lvl.index_at(1, 4).unwrap() as u16;

        let mut base = ScoreElement::default();
        let mut scores = [ScoreElement::default()];
        compute_base_distance(
            &lvl,
            &distances,
            &lvl.board,
            &mut base,
            &moves(base_index, moved),
            &mut scores,
        );

        assert_eq!(base.dist_to_targets, 0);
        assert_eq!(scores[0].dist_to_targets, 1);
    }

    #[test]
    fn imagined_distance_uses_the_plan_boxes() {
        let lvl = level("#######\n#@ $ .#\n#######\n");
        let distances = crate::distance::compute(&lvl);
        let mut imagined = lvl.board;
        imagined.clear_boxes_inplace();
        imagined.or(1, 5, BOX);

        let from = lvl.index_at(1, 3).unwrap() as u16;
        let mut base = ScoreElement::default();
        let mut scores = [ScoreElement::default()];
        compute_imagine_distance(
            &lvl,
            &distances,
            &lvl.board,
            &imagined,
            &mut base,
            &moves(from, from),
            &mut scores,
            SearchMode::Normal,
        );

        assert_eq!(base.dist_to_imagined, 2);
        assert_eq!(scores[0].dist_to_imagined, 2);
    }

    #[test]
    fn imagined_distance_is_skipped_for_girl() {
        let lvl = level("#######\n#@ $ .#\n#######\n");
        let distances = crate::distance::compute(&lvl);
        let mut base = ScoreElement::default();
        let mut scores = [ScoreElement::default()];
        compute_imagine_distance(
            &lvl,
            &distances,
            &lvl.board,
            &lvl.board,
            &mut base,
            &moves(0, 0),
            &mut scores,
            SearchMode::GirlSearch,
        );
        assert_eq!(base.dist_to_imagined, 0);
        assert_eq!(scores[0].dist_to_imagined, 0);
    }
}
