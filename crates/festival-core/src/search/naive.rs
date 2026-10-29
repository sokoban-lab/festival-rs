//! Naive scoring used by strategy H's forward search (`naive.cpp`).

use crate::analysis::match_distance::{compute_base_distance, compute_match_distance};
use crate::error::{Result, SolveError};
use crate::search::tree::{ExpansionId, Tree};
use crate::solver::LevelContext;

/// `set_naive_data_in_node`: refresh the node and son distances.
pub fn set_naive_data_in_node(
    tree: &mut Tree,
    ctx: &LevelContext,
    expansion: ExpansionId,
) -> Result<()> {
    let (node, start, count) = {
        let e = &tree.expansions[expansion as usize];
        (e.node, e.move_hash_place, e.moves_num as usize)
    };

    let mut moves = Vec::with_capacity(count);
    let mut scores = Vec::with_capacity(count);
    for i in 0..count {
        let move_hash = tree.move_hashes[start + i];
        moves.push(move_hash.move_data);
        scores.push(
            tree.get_score_of_hash(move_hash.hash)
                .ok_or_else(|| SolveError::Internal("missing node".into()))?,
        );
    }

    let board = tree.expansion_board(expansion)?;
    let mut base_score = tree.nodes[node as usize].score;

    if tree.pull_mode {
        compute_base_distance(
            &ctx.level,
            &ctx.distances,
            &board,
            &mut base_score,
            &moves,
            &mut scores,
        );
    } else {
        compute_match_distance(
            &ctx.level,
            &ctx.distances,
            &board,
            &mut base_score,
            &moves,
            &mut scores,
        );
    }

    tree.nodes[node as usize].score = base_score;

    for (i, score) in scores.iter().enumerate() {
        let hash = tree.move_hashes[start + i].hash;
        let place = tree
            .find_node_by_hash(hash)
            .ok_or_else(|| SolveError::Internal("missing node".into()))?;
        tree.nodes[place as usize].score.dist_to_targets = score.dist_to_targets;
    }

    Ok(())
}

/// `fill_naive_scores`.
pub fn fill_naive_scores(initial: &crate::board::Board, board: &crate::board::Board) -> i16 {
    board.boxes_on_bases(initial) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;

    #[test]
    fn naive_counts_boxes_on_bases() {
        let initial = Board::from_rows(&[b"#######", b"#@ $ .#", b"#######"]).unwrap();
        assert_eq!(fill_naive_scores(&initial, &initial), 1);

        let mut board = initial;
        board.and_not(1, 3, crate::constants::BOX);
        board.or(1, 4, crate::constants::BOX);
        assert_eq!(fill_naive_scores(&initial, &board), 0);
    }
}
