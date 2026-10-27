// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! `MAX_DIST_SEARCH` per-node Hungarian distances (`max_dist.cpp`).

use crate::analysis::match_distance::compute_match_distance;
use crate::constants::SearchMode;
use crate::search::score::ScoreElement;
use crate::search::tree::Tree;
use crate::solver::LevelContext;

/// Replaces `dist_to_targets` with the assignment distance for this node and
/// its children. Only meaningful in pull mode (MAX_DIST searches).
pub fn set_max_dist_data_in_node(
    tree: &mut Tree,
    ctx: &LevelContext,
    expansion: u32,
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

    let mut moves = Vec::with_capacity(count);
    let mut scores: Vec<ScoreElement> = Vec::with_capacity(count);
    for i in 0..count {
        let mv = tree.move_hashes[start + i].move_data;
        moves.push(mv);
        scores.push(
            tree.get_score_of_hash(tree.move_hashes[start + i].hash)
                .unwrap_or_default(),
        );
    }

    let mut base_score = tree.nodes[node as usize].score;
    compute_match_distance(
        &ctx.level,
        &ctx.distances,
        &board,
        &mut base_score,
        &moves,
        &mut scores,
    );
    tree.nodes[node as usize].score = base_score;

    for (i, score) in scores.iter().enumerate() {
        let hash = tree.move_hashes[start + i].hash;
        let Some(place) = tree.find_node_by_hash(hash) else {
            continue;
        };
        tree.nodes[place as usize].score.dist_to_targets = score.dist_to_targets;
    }

    Ok(())
}
