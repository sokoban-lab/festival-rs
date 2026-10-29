//! Push/pull distance matrices (`distance.cpp`).

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::graph::Graph;
use crate::level::Level;

pub struct Distances {
    n: usize,
    from_to: Vec<u32>,
    bfs_from_to: Vec<u32>,
    impossible_place: Vec<bool>,
}

impl Distances {
    pub fn index_num(&self) -> usize {
        self.n
    }

    #[inline]
    pub fn from_to(&self, from: usize, to: usize) -> u32 {
        self.from_to[from * self.n + to]
    }

    #[inline]
    pub fn bfs_from_to(&self, from: usize, to: usize) -> u32 {
        self.bfs_from_to[from * self.n + to]
    }

    #[inline]
    pub fn impossible(&self, index: usize) -> bool {
        self.impossible_place[index]
    }

    pub fn impossible_slice(&self) -> &[bool] {
        &self.impossible_place
    }

    pub fn from_to_slice(&self) -> &[u32] {
        &self.from_to
    }

    pub fn bfs_from_to_slice(&self) -> &[u32] {
        &self.bfs_from_to
    }
}

pub fn compute(level: &Level) -> Distances {
    let n = level.index_num();
    let board = level.board;
    let without_boxes = board.without_boxes();
    let mut graph = Graph::new(level);
    graph.build(level, &without_boxes, false);

    let mut from_to = vec![INF; n * n];
    for from in 0..n {
        graph.set_weights_infinity();
        graph.clear_weight_around_cell(from);
        graph.do_iterations(false);
        for to in 0..n {
            from_to[from * n + to] = graph.get_weight_around_cell(to);
        }
    }

    let mut can_get_to_dest = vec![false; n];
    let mut can_get_from_start = vec![false; n];

    for i in 0..n {
        for y in 0..board.height() {
            for x in 0..board.width() {
                if !level.is_inner(y, x) {
                    continue;
                }
                if board.cell(y, x) & TARGET != 0 {
                    let Some(place) = level.index_at(y, x) else {
                        continue;
                    };
                    if from_to[i * n + place] < INF {
                        can_get_to_dest[i] = true;
                    }
                }
            }
        }
    }

    for i in 0..n {
        for y in 0..board.height() {
            for x in 0..board.width() {
                if !level.is_inner(y, x) {
                    continue;
                }
                if board.cell(y, x) & BOX != 0 {
                    let Some(place) = level.index_at(y, x) else {
                        continue;
                    };
                    if from_to[place * n + i] < INF {
                        can_get_from_start[i] = true;
                    }
                }
            }
        }
    }

    let mut impossible_place = vec![true; MAX_INNER];
    for i in 0..n {
        if can_get_from_start[i] && can_get_to_dest[i] {
            impossible_place[i] = false;
        }
    }

    let mut bfs_from_to = vec![INF; n * n];
    let mut dist = [INF; MAX_INNER];
    for i in 0..n {
        let (y, x) = level.coord(i);
        bfs::bfs(&board, &mut dist, y, x, true);
        for j in 0..n {
            let (yy, xx) = level.coord(j);
            bfs_from_to[i * n + j] = dist[Board::flat_index(yy, xx)];
        }
    }

    Distances {
        n,
        from_to,
        bfs_from_to,
        impossible_place,
    }
}

pub fn get_distance_from_targets(level: &Level, board: &Board, dist: &mut [u32]) {
    let mut target_area = Board::new(board.width(), board.height()).expect("same dims");
    let without_boxes = board.without_boxes();

    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & TARGET != 0 {
                target_area.set(y, x, 1);
            }
        }
    }

    let mut graph = Graph::new(level);
    graph.find_distance_from_a_group(level, &without_boxes, &target_area, dist, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn push_distances_along_corridor() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let distances = compute(&level);
        let box_index = level.index_at(1, 3).unwrap();
        let target = level.index_at(1, 5).unwrap();
        assert_eq!(distances.from_to(box_index, target), 2);
        assert!(!distances.impossible(box_index));
        assert!(!distances.impossible(target));
    }

    #[test]
    fn wall_positions_are_impossible() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let distances = compute(&level);
        assert!(distances.impossible(MAX_INNER - 1));
    }

    #[test]
    fn bfs_distance_ignores_boxes() {
        let level = level("#######\n#@ $ .#\n#######\n");
        let distances = compute(&level);
        let start = level.index_at(1, 1).unwrap();
        let target = level.index_at(1, 5).unwrap();
        assert_eq!(distances.bfs_from_to(start, target), 4);
    }
}
