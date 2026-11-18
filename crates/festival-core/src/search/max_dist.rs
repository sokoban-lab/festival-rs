//! `MAX_DIST_SEARCH` per-node Hungarian distances (`max_dist.cpp`).

use crate::analysis::match_distance::compute_match_distance;
use crate::constants::SearchMode;
use crate::search::helper::Helper;
use crate::search::tree::Tree;
use crate::solver::LevelContext;

/// Replaces `dist_to_targets` with the assignment distance for this node and
/// its children. Only meaningful in pull mode (MAX_DIST searches).
pub fn set_max_dist_data_in_node(
    tree: &mut Tree,
    ctx: &LevelContext,
    expansion: u32,
    helper: &mut Helper,
) -> crate::error::Result<()> {
    if !tree.pull_mode {
        return Ok(());
    }
    if !matches!(
        tree.search_mode,
        SearchMode::MaxDistSearch | SearchMode::MaxDistSearch2
    ) {
        return Ok(());
    }

    let (node, start, count) = {
        let e = &tree.expansions[expansion as usize];
        (e.node, e.move_hash_place, e.moves_num as usize)
    };
    let board = tree.expansion_board(expansion)?;

    helper.advisor_moves.clear();
    helper.advisor_scores.clear();
    helper.advisor_moves.reserve(count);
    helper.advisor_scores.reserve(count);
    for i in 0..count {
        let move_hash = tree.move_hashes[start + i];
        helper.advisor_moves.push(move_hash.move_data);
        helper
            .advisor_scores
            .push(tree.nodes[move_hash.node as usize].score);
    }

    let mut base_score = tree.nodes[node as usize].score;
    compute_match_distance(
        &ctx.level,
        &ctx.distances,
        &board,
        &mut base_score,
        &helper.advisor_moves,
        &mut helper.advisor_scores,
    );
    tree.nodes[node as usize].score = base_score;

    for (i, score) in helper.advisor_scores.iter().enumerate() {
        let place = tree.move_hashes[start + i].node;
        tree.nodes[place as usize].score.dist_to_targets = score.dist_to_targets;
    }

    Ok(())
}
