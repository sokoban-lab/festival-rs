// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Cycle deadlock detection inside the search tree (`cycle_deadlock.cpp`).

use crate::constants::SearchMode;
use crate::search::tree::{ExpansionId, NONE_EXPANSION, Tree};
use crate::solver::LevelContext;

fn set_leaf_direction_in_tree(tree: &mut Tree) {
    for expansion in &mut tree.expansions {
        expansion.leaf_direction = -1;
    }

    let mut changed = true;
    while changed {
        changed = false;

        for i in (0..tree.expansions.len()).rev() {
            if tree.expansions[i].leaf_direction != -1 {
                continue;
            }
            let node = tree.expansions[i].node;
            if tree.nodes[node as usize].deadlocked {
                continue;
            }

            let (start, count) = {
                let e = &tree.expansions[i];
                (e.move_hash_place, e.moves_num as usize)
            };

            for j in 0..count {
                if tree.move_hashes[start + j].deadlocked {
                    continue;
                }
                let hash = tree.move_hashes[start + j].hash;
                let Some(place) = tree.find_node_by_hash(hash) else {
                    continue;
                };
                if tree.nodes[place as usize].deadlocked {
                    tree.move_hashes[start + j].deadlocked = true;
                    continue;
                }
                if tree.nodes[place as usize].expansion == NONE_EXPANSION {
                    tree.expansions[i].leaf_direction = j as i32;
                    changed = true;
                    break;
                }
                let child = tree.nodes[place as usize].expansion;
                if tree.expansions[child as usize].leaf_direction != -1 {
                    tree.expansions[i].leaf_direction = j as i32;
                    changed = true;
                    break;
                }
            }
        }
    }

    for i in 0..tree.expansions.len() {
        let node = tree.expansions[i].node;
        if tree.nodes[node as usize].deadlocked {
            continue;
        }
        if tree.expansions[i].leaf_direction != -1 {
            continue;
        }
        tree.nodes[node as usize].deadlocked = true;
    }
}

/// Port of `check_cycle_deadlock`.
pub fn check_cycle_deadlock(tree: &mut Tree, mut expansion: ExpansionId, ctx: &LevelContext) {
    if tree.search_mode == SearchMode::BaseSearch {
        return;
    }
    if !ctx.level.is_cyclic() {
        return;
    }

    let mut depth = 0;
    while tree.expansions[expansion as usize].leaf_direction != -1 {
        let node = tree.expansions[expansion as usize].node;
        if tree.nodes[node as usize].deadlocked {
            break;
        }

        let (start, leaf) = {
            let e = &tree.expansions[expansion as usize];
            (e.move_hash_place, e.leaf_direction as usize)
        };
        let hash = tree.move_hashes[start + leaf].hash;
        let Some(place) = tree.find_node_by_hash(hash) else {
            break;
        };
        if tree.nodes[place as usize].deadlocked {
            break;
        }
        if tree.nodes[place as usize].expansion == NONE_EXPANSION {
            return;
        }
        expansion = tree.nodes[place as usize].expansion;
        depth += 1;
        if depth > 500 {
            break;
        }
    }

    set_leaf_direction_in_tree(tree);
}
