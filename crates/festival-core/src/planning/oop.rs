// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Out-of-plan zones and advisor (`oop.cpp`).

use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::graph::Graph;
use crate::moves::Move;
use crate::planning::OopData;
use crate::search::helper::Helper;
use crate::search::score::ScoreElement;
use crate::solver::LevelContext;

fn get_parked_boxes(ctx: &LevelContext, step: usize, helper: &Helper) -> Board {
    let mut parked =
        Board::new(ctx.level.initial.width(), ctx.level.initial.height()).expect("same dims");
    let init_num = helper
        .parking_order
        .iter()
        .filter(|entry| entry.from == -2)
        .count();
    let step = step.max(init_num);

    for i in 0..step.min(helper.parking_order.len()) {
        let entry = helper.parking_order[i];
        if (entry.until == -1 || entry.until as usize >= step) && entry.to >= 0 {
            let (y, x) = ctx.level.coord(entry.to as usize);
            parked.set(y, x, 1);
        }
    }
    parked
}

fn get_oop_zone_for_step(ctx: &LevelContext, step: usize, helper: &Helper) -> Result<Board> {
    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let mut parked_boxes = Board::new(width, height)?;
    let mut next_targets = Board::new(width, height)?;

    for entry in helper.parking_order.iter().take(step + 1) {
        if entry.until == -1 && entry.to >= 0 {
            let (y, x) = ctx.level.coord(entry.to as usize);
            parked_boxes.set(y, x, 1);
        }
    }

    for entry in helper.parking_order.iter().skip(step + 1) {
        if entry.to >= 0 {
            let (y, x) = ctx.level.coord(entry.to as usize);
            next_targets.set(y, x, 1);
        }
    }

    let mut b = ctx.level.initial;
    b.clear_boxes_inplace();
    for y in 0..height {
        for x in 0..width {
            if parked_boxes.cell(y, x) != 0 {
                b.set(y, x, WALL);
            }
        }
    }

    let mut graph = Graph::new(&ctx.level);
    let mut sources = Board::new(width, height)?;
    graph.find_sources_of_a_board_input(&ctx.level, &b, &next_targets, &mut sources);

    let mut oop = Board::new(width, height)?;
    for y in 0..height {
        for x in 0..width {
            if !ctx.level.is_inner(y, x) {
                continue;
            }
            if b.cell(y, x) == WALL {
                continue;
            }
            let Some(index) = ctx.level.index_at(y, x) else {
                continue;
            };
            if ctx.distances.impossible(index) {
                continue;
            }
            if sources.cell(y, x) != 0 {
                continue;
            }
            oop.set(y, x, 1);
        }
    }
    Ok(oop)
}

fn get_oop_distance_for_step(ctx: &LevelContext, step: usize, helper: &Helper) -> Result<Vec<u32>> {
    let zones = helper
        .oop
        .as_ref()
        .ok_or_else(|| SolveError::Internal("missing oop".into()))?;
    let zone = zones.zones.get(step).copied().unwrap_or_else(|| {
        Board::new(ctx.level.initial.width(), ctx.level.initial.height()).unwrap()
    });

    let parked = get_parked_boxes(ctx, step, helper);
    let mut empty = ctx.level.initial;
    empty.clear_boxes_inplace();
    for y in 0..empty.height() {
        for x in 0..empty.width() {
            if parked.cell(y, x) != 0 {
                empty.set(y, x, WALL);
            }
        }
    }

    let mut dist = vec![INF; MAX_INNER];
    let mut graph = Graph::new(&ctx.level);
    graph.find_distance_from_a_group(&ctx.level, &empty, &zone, &mut dist, false);
    Ok(dist)
}

/// Port of `prepare_oop_zones`.
pub fn prepare_oop_zones(ctx: &LevelContext, helper: &mut Helper) -> Result<()> {
    let n = helper.parking_order.len();
    if n < 2 {
        helper.oop = Some(OopData::default());
        return Ok(());
    }

    let width = ctx.level.initial.width();
    let height = ctx.level.initial.height();
    let mut zones = vec![Board::new(width, height)?; n - 1];
    for (i, zone) in zones.iter_mut().enumerate() {
        *zone = get_oop_zone_for_step(ctx, i, helper)?;
    }

    // The zone computed for step i is the last chance to clear boxes; copy it
    // backwards so the score reacts as early as possible.
    let mut current = Board::new(width, height)?;
    let mut last_modified = 0usize;
    for i in 0..n - 1 {
        if zones[i] == current {
            continue;
        }
        current = zones[i];
        for zone in zones.iter_mut().take(i + 1).skip(last_modified) {
            *zone = current;
        }
        last_modified = i;
    }

    helper.oop = Some(OopData {
        zones,
        distances: Vec::new(),
    });
    let mut distances = Vec::with_capacity(n - 1);
    for i in 0..n - 1 {
        distances.push(get_oop_distance_for_step(ctx, i, helper)?);
    }
    if let Some(oop) = helper.oop.as_mut() {
        oop.distances = distances;
    }
    Ok(())
}

fn count_boxes_in_oop_zone(board: &Board, oop: &Board) -> i32 {
    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if oop.cell(y, x) != 0 && board.cell(y, x) & BOX != 0 {
                sum += 1;
            }
        }
    }
    sum
}

/// Port of `get_out_of_plan_score`.
pub fn get_out_of_plan_score(board: &Board, packed_num: i32, helper: &Helper) -> i32 {
    let Some(oop) = helper.oop.as_ref() else {
        return 0;
    };
    if packed_num < 0 {
        return 0;
    }
    let Some(zone) = oop.zones.get(packed_num as usize) else {
        return 0;
    };
    count_boxes_in_oop_zone(board, zone)
}

/// Port of `new_oop_advisor`.
pub fn new_oop_advisor(
    ctx: &LevelContext,
    _board: &Board,
    base_score: &ScoreElement,
    moves: &[Move],
    scores: &[ScoreElement],
    _pull_mode: bool,
    helper: &Helper,
) -> i32 {
    if base_score.out_of_plan == 0 {
        return -1;
    }
    let Some(oop) = helper.oop.as_ref() else {
        return -1;
    };
    let step = base_score.packed_boxes;
    if step < 0 {
        return -1;
    }
    let Some(distances) = oop.distances.get(step as usize) else {
        return -1;
    };

    let mut best_move = -1i32;
    let mut min_from = 1_000_000i32;
    let mut max_to = -1i32;
    let min_connect = base_score.connectivity;
    let min_rooms = base_score.rooms_score;

    for (i, mv) in moves.iter().enumerate() {
        let score = &scores[i];
        if score.connectivity > min_connect {
            continue;
        }
        if score.rooms_score > min_rooms {
            continue;
        }
        if score.out_of_plan > base_score.out_of_plan {
            continue;
        }
        if score.packed_boxes < base_score.packed_boxes {
            continue;
        }

        if ctx.distances.from_to(mv.to as usize, mv.from as usize) == INF {
            continue;
        }

        let (from_y, from_x) = ctx.level.coord(mv.from as usize);
        let (to_y, to_x) = ctx.level.coord(mv.to as usize);
        let from_dist = distances[Board::flat_index(from_y, from_x)] as i32;
        let to_dist = distances[Board::flat_index(to_y, to_x)] as i32;

        if from_dist == INF as i32 || to_dist == INF as i32 {
            continue;
        }
        if to_dist <= from_dist {
            continue;
        }
        if from_dist > min_from {
            continue;
        }

        if from_dist < min_from {
            min_from = from_dist;
            max_to = to_dist;
            best_move = i as i32;
        } else if from_dist == min_from && to_dist > max_to {
            max_to = to_dist;
            best_move = i as i32;
        }
    }

    best_move
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;
    use crate::level::Level;
    use std::time::Duration;

    fn context(text: &str) -> LevelContext {
        let level = Level::preprocess(parse_levels(text).remove(0)).unwrap();
        let clock = crate::solver::SearchClock::new(Some(Duration::from_secs(60)), None);
        LevelContext::analyse(level, &clock).unwrap()
    }

    #[test]
    fn oop_zones_are_built_for_each_plan_step() {
        let ctx = context("########\n#@ $ . #\n#      #\n# $ .  #\n########\n");
        let mut helper = Helper {
            parking_order: vec![
                crate::planning::ParkOrder {
                    from: -2,
                    to: ctx.level.index_at(1, 3).unwrap() as i32,
                    until: -1,
                },
                crate::planning::ParkOrder {
                    from: -1,
                    to: ctx.level.index_at(1, 5).unwrap() as i32,
                    until: -1,
                },
                crate::planning::ParkOrder {
                    from: -1,
                    to: ctx.level.index_at(3, 5).unwrap() as i32,
                    until: -1,
                },
            ],
            ..Helper::default()
        };
        crate::planning::park_order::recompute_until(&mut helper.parking_order);
        prepare_oop_zones(&ctx, &mut helper).unwrap();
        let oop = helper.oop.as_ref().unwrap();
        assert_eq!(oop.zones.len(), 2);
        assert_eq!(oop.distances.len(), 2);
    }

    #[test]
    fn score_counts_boxes_out_of_plan() {
        let ctx = context("#######\n#@ $ .#\n#######\n");
        let mut helper = Helper::default();
        let zone = ctx.level.initial;
        helper.oop = Some(OopData {
            zones: vec![zone],
            distances: vec![vec![INF; MAX_INNER]],
        });
        let score = get_out_of_plan_score(&ctx.level.board, 0, &helper);
        assert!(score >= 0);
    }
}
