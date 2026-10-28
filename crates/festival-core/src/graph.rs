// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Box move graph (`graph.cpp`).
//!
//! Vertex index is `box_index * 4 + direction`, where direction is the side of
//! the box the player stands on. 0 is kept as the C++ null sentinel.

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::static_checks::mark_deadlock_2x2;
use crate::level::Level;
use crate::moves::Move;

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub shift: [u32; 4],
    pub push: u32,
    pub pull: u32,
    pub weight: u32,
    pub active: bool,
    pub src: i32,
    pub reversible: bool,
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            shift: [0; 4],
            push: 0,
            pull: 0,
            weight: INF,
            active: false,
            src: -1,
            reversible: false,
        }
    }
}

#[derive(Debug)]
pub struct Graph {
    pub vertices: Vec<Vertex>,
    pub current_impossible: Board,
    pub vertices_num: usize,
}

impl Graph {
    pub fn new(level: &Level) -> Self {
        let vertices_num = level.index_num() * 4;
        Self {
            vertices: vec![Vertex::default(); vertices_num],
            current_impossible: Board::new(level.initial.width(), level.initial.height())
                .expect("level dimensions"),
            vertices_num,
        }
    }

    /// Like `new`, but leaves the vertex array uninitialized; `build` fills it.
    pub fn unbuilt(level: &Level) -> Self {
        Self {
            vertices: Vec::with_capacity(level.index_num() * 4),
            current_impossible: Board::new(level.initial.width(), level.initial.height())
                .expect("level dimensions"),
            vertices_num: 0,
        }
    }

    /// True when this graph was sized for `level`'s geometry.
    pub fn matches_level(&self, level: &Level) -> bool {
        self.current_impossible.width() == level.initial.width()
            && self.current_impossible.height() == level.initial.height()
            && self.vertices.len() == level.index_num() * 4
    }

    pub fn build(&mut self, level: &Level, board: &Board, pull_mode: bool) {
        debug_assert!(!board.contains_boxes());
        self.vertices_num = level.index_num() * 4;
        self.vertices.clear();
        self.vertices.resize(
            self.vertices_num,
            Vertex {
                weight: INF,
                src: -1,
                ..Vertex::default()
            },
        );

        self.init_active(level, board);
        self.init_shift(level, board);
        if pull_mode {
            self.init_pull(level, board);
        } else {
            self.init_push(level, board);
        }
        self.set_weights_infinity();
    }

    fn init_active(&mut self, level: &Level, board: &Board) {
        for i in 0..level.index_num() {
            let (base_y, base_x) = level.coord(i);
            if board.cell(base_y, base_x) & WALL != 0 {
                for dir in 0..4 {
                    self.vertices[i * 4 + dir].active = false;
                }
                continue;
            }
            for dir in 0..4 {
                let y = base_y as isize + DELTA_Y[dir];
                let x = base_x as isize + DELTA_X[dir];
                self.vertices[i * 4 + dir].active = board.get(y, x) & WALL == 0;
            }
        }
    }

    fn init_shift(&mut self, level: &Level, board: &Board) {
        for vertex in &mut self.vertices {
            vertex.shift = [0; 4];
        }

        let mut zones = [INF; MAX_INNER];
        let mut temp = *board;

        for i in 0..level.index_num() {
            let (y, x) = level.coord(i);
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }

            temp.set(y, x, board.cell(y, x) | BOX);
            bfs::mark_connectivities_around_place(level, &temp, &mut zones, y, x);
            temp.set(y, x, board.cell(y, x));

            for from in 0..4 {
                if !self.vertices[i * 4 + from].active {
                    continue;
                }
                let from_y = y as isize + DELTA_Y[from];
                let from_x = x as isize + DELTA_X[from];
                if from_y < 0 || from_x < 0 {
                    continue;
                }
                let from_idx = Board::flat_index(from_y as usize, from_x as usize);
                if zones[from_idx] == INF {
                    continue;
                }
                for to in 0..4 {
                    if to == from || !self.vertices[i * 4 + to].active {
                        continue;
                    }
                    let to_y = y as isize + DELTA_Y[to];
                    let to_x = x as isize + DELTA_X[to];
                    if to_y < 0 || to_x < 0 {
                        continue;
                    }
                    let to_idx = Board::flat_index(to_y as usize, to_x as usize);
                    if zones[to_idx] == INF {
                        continue;
                    }
                    if zones[from_idx] == zones[to_idx] {
                        self.vertices[i * 4 + from].shift[to] = (i * 4 + to) as u32;
                    }
                }
            }
        }
    }

    fn init_push(&mut self, level: &Level, board: &Board) {
        for i in 0..self.vertices_num {
            if !self.vertices[i].active {
                continue;
            }
            let (base_y, base_x) = level.coord(i / 4);
            debug_assert!(board.cell(base_y, base_x) & OCCUPIED == 0);
            let dir = i % 4;
            let push_y = base_y as isize - DELTA_Y[dir];
            let push_x = base_x as isize - DELTA_X[dir];

            if board.get(push_y, push_x) & OCCUPIED != 0 {
                self.vertices[i].push = 0;
            } else {
                self.vertices[i].push = match (push_y, push_x) {
                    (y, x) if y >= 0 && x >= 0 => level
                        .index_at(y as usize, x as usize)
                        .map(|new_index| (new_index * 4 + dir) as u32)
                        .unwrap_or(0),
                    _ => 0,
                };
            }
        }
    }

    fn init_pull(&mut self, level: &Level, board: &Board) {
        for i in 0..self.vertices_num {
            if !self.vertices[i].active {
                continue;
            }
            let (base_y, base_x) = level.coord(i / 4);
            debug_assert!(board.cell(base_y, base_x) & OCCUPIED == 0);
            let dir = i % 4;
            let pull_y = base_y as isize + DELTA_Y[dir];
            let pull_x = base_x as isize + DELTA_X[dir];
            let sok_y = base_y as isize + DELTA_Y[dir] * 2;
            let sok_x = base_x as isize + DELTA_X[dir] * 2;

            self.vertices[i].pull = 0;
            if board.get(sok_y, sok_x) & OCCUPIED != 0 {
                continue;
            }
            if pull_y >= 0 && pull_x >= 0 {
                if let Some(new_index) = level.index_at(pull_y as usize, pull_x as usize) {
                    self.vertices[i].pull = (new_index * 4 + dir) as u32;
                }
            }
        }
    }

    pub fn set_weights_infinity(&mut self) {
        for vertex in &mut self.vertices {
            vertex.weight = INF;
            vertex.src = -1;
            vertex.reversible = false;
        }
    }

    pub fn clear_weight_around_cell(&mut self, index: usize) {
        for dir in 0..4 {
            if self.vertices[index * 4 + dir].active {
                self.vertices[index * 4 + dir].weight = 0;
            }
        }
    }

    pub fn do_iterations(&mut self, pull_mode: bool) {
        let n = self.vertices_num;
        let mut visited = vec![false; n];
        let mut current: Vec<u32> = Vec::with_capacity(n);
        let mut next: Vec<u32> = Vec::with_capacity(n);

        for (i, vertex) in self.vertices.iter_mut().enumerate() {
            if vertex.weight == 0 {
                vertex.src = -1;
                next.push(i as u32);
                visited[i] = true;
            }
        }

        while !next.is_empty() {
            current.clear();
            std::mem::swap(&mut current, &mut next);

            for &v in &current {
                let vertex = self.vertices[v as usize];

                for dir in 0..4 {
                    let next_vertex = vertex.shift[dir];
                    if next_vertex == 0 {
                        continue;
                    }
                    debug_assert!(self.vertices[next_vertex as usize].active);
                    if visited[next_vertex as usize] {
                        continue;
                    }
                    visited[next_vertex as usize] = true;
                    let target = &mut self.vertices[next_vertex as usize];
                    target.weight = vertex.weight;
                    target.src = v as i32;
                    next.push(next_vertex);
                }

                let next_vertex = if pull_mode { vertex.pull } else { vertex.push };
                if next_vertex == 0 {
                    continue;
                }
                debug_assert!(self.vertices[next_vertex as usize].active);
                if visited[next_vertex as usize] {
                    continue;
                }
                visited[next_vertex as usize] = true;
                let target = &mut self.vertices[next_vertex as usize];
                target.weight = vertex.weight + 1;
                target.src = v as i32;
                next.push(next_vertex);
            }
        }
    }

    pub fn get_weight_around_cell(&self, index: usize) -> u32 {
        let mut min = INF;
        for dir in 0..4 {
            let vertex = &self.vertices[index * 4 + dir];
            if !vertex.active {
                continue;
            }
            if vertex.weight < min {
                min = vertex.weight;
            }
        }
        min
    }

    pub fn mark_reversible_nodes(&mut self, pull_mode: bool) {
        self.do_iterations(!pull_mode);
        for vertex in &mut self.vertices {
            if vertex.weight < INF {
                vertex.reversible = true;
            }
            if vertex.weight != 0 {
                vertex.weight = INF;
            }
        }
    }

    pub fn get_box_moves(
        &mut self,
        level: &Level,
        start_index: usize,
        moves: &mut [Move],
    ) -> usize {
        let mut count = 0;
        for i in 0..level.index_num() {
            let (y, x) = level.coord(i);
            if self.current_impossible.cell(y, x) != 0 {
                continue;
            }
            for dir in 0..4 {
                let vertex = self.vertices[i * 4 + dir];
                if vertex.weight == INF || vertex.weight == 0 {
                    continue;
                }
                let src = vertex.src;
                if src < 0 {
                    continue;
                }
                if src / 4 == i as i32 {
                    continue;
                }
                moves[count] = Move {
                    from: start_index as u16,
                    to: i as u16,
                    sokoban_position: dir as u8,
                    kill: false,
                    base: false,
                    pull: false,
                    attr: crate::moves::MoveAttr {
                        weight: 0,
                        advisor: false,
                        reversible: vertex.reversible,
                    },
                };
                count += 1;
            }
        }
        count
    }

    fn in_group(index: usize, group: &[usize]) -> bool {
        group.contains(&index)
    }

    pub fn find_sources_of_a_group(
        &mut self,
        level: &Level,
        board: &Board,
        group: &[usize],
        res: &mut Board,
    ) -> usize {
        self.build(level, board, true);
        for &cell in group {
            self.clear_weight_around_cell(cell);
        }
        self.do_iterations(true);

        let mut sources = Vec::with_capacity(level.index_num());
        for i in 0..level.index_num() {
            if self.get_weight_around_cell(i) != INF || Self::in_group(i, group) {
                sources.push(i);
            }
        }
        res.zero();
        for &index in &sources {
            let (y, x) = level.coord(index);
            res.set(y, x, 1);
        }
        sources.len()
    }

    pub fn find_targets_of_a_group(
        &mut self,
        level: &Level,
        board: &Board,
        group: &[usize],
        targets: &mut Board,
    ) -> usize {
        self.build(level, board, false);
        for &cell in group {
            self.clear_weight_around_cell(cell);
        }
        self.do_iterations(false);

        let mut found = Vec::with_capacity(level.index_num());
        for i in 0..level.index_num() {
            if self.get_weight_around_cell(i) != INF || Self::in_group(i, group) {
                found.push(i);
            }
        }
        targets.zero();
        for &index in &found {
            let (y, x) = level.coord(index);
            targets.set(y, x, 1);
        }
        found.len()
    }

    pub fn find_targets_of_a_board_input(
        &mut self,
        level: &Level,
        board: &Board,
        input: &Board,
        targets: &mut Board,
    ) -> usize {
        let mut group = Vec::new();
        for y in 0..input.height() {
            for x in 0..input.width() {
                if !level.is_inner(y, x) || input.cell(y, x) == 0 {
                    continue;
                }
                if let Some(index) = level.index_at(y, x) {
                    group.push(index);
                }
            }
        }
        self.find_targets_of_a_group(level, board, &group, targets)
    }

    pub fn find_sources_of_a_board_input(
        &mut self,
        level: &Level,
        board: &Board,
        input: &Board,
        sources: &mut Board,
    ) -> usize {
        let mut group = Vec::new();
        for y in 0..input.height() {
            for x in 0..input.width() {
                if input.cell(y, x) == 0 {
                    continue;
                }
                if let Some(index) = level.index_at(y, x) {
                    group.push(index);
                }
            }
        }
        self.find_sources_of_a_group(level, board, &group, sources)
    }

    pub fn find_targets_of_place_without_init_graph(
        &mut self,
        level: &Level,
        start_y: usize,
        start_x: usize,
        targets: &mut Board,
    ) {
        targets.zero();
        self.set_weights_infinity();
        if let Some(start_index) = level.index_at(start_y, start_x) {
            self.clear_weight_around_cell(start_index);
        }
        self.do_iterations(false);

        for i in 0..self.vertices_num {
            if self.vertices[i].weight != INF {
                let (y, x) = level.coord(i / 4);
                targets.set(y, x, 1);
            }
        }
        targets.set(start_y, start_x, 1);
    }

    pub fn find_distance_from_a_group(
        &mut self,
        level: &Level,
        board: &Board,
        group: &Board,
        dist: &mut [u32],
        pull_mode: bool,
    ) {
        self.build(level, board, pull_mode);
        for i in 0..level.index_num() {
            let (y, x) = level.coord(i);
            if group.cell(y, x) != 0 {
                self.clear_weight_around_cell(i);
            }
        }
        self.do_iterations(pull_mode);

        bfs::init_dist(dist);
        for i in 0..level.index_num() {
            let (y, x) = level.coord(i);
            dist[Board::flat_index(y, x)] = self.get_weight_around_cell(i);
        }
    }

    fn clear_shift(&mut self) {
        for vertex in &mut self.vertices {
            vertex.shift = [0; 4];
        }
    }

    fn update_shift(&mut self, level: &Level, board: &Board, box_y: usize, box_x: usize) {
        let Some(box_index) = level.index_at(box_y, box_x) else {
            return;
        };
        let base = box_index * 4;
        let mut has_sokoban = [false; 4];
        for (dir, has) in has_sokoban.iter_mut().enumerate() {
            if board.at_dir(box_y, box_x, dir) & SOKOBAN != 0 {
                *has = true;
            }
        }
        for (from, has_from) in has_sokoban.iter().enumerate() {
            if !has_from {
                continue;
            }
            for (to, has_to) in has_sokoban.iter().enumerate() {
                if *has_to {
                    self.vertices[base + from].shift[to] = (base + to) as u32;
                }
            }
        }
    }

    pub fn mark_box_moves_in_graph(
        &mut self,
        level: &Level,
        board_in: &Board,
        box_index: usize,
        pull_mode: bool,
    ) {
        let mut b = *board_in;
        self.clear_shift();
        self.set_weights_infinity();

        let (box_y, box_x) = level.coord(box_index);
        debug_assert!(b.cell(box_y, box_x) & BOX != 0);

        let mut current: Vec<u32> = Vec::with_capacity(MAX_INNER);
        let mut next: Vec<u32> = Vec::with_capacity(MAX_INNER);

        for dir in 0..4 {
            let sy = box_y as isize + DELTA_Y[dir];
            let sx = box_x as isize + DELTA_X[dir];
            if b.get(sy, sx) & SOKOBAN != 0 {
                self.vertices[box_index * 4 + dir].weight = 0;
                current.push((box_index * 4 + dir) as u32);
            }
        }

        b.and_not(box_y, box_x, BOX);
        let mut d = 0u32;

        while !current.is_empty() {
            d += 1;
            next.clear();

            for &index in &current {
                let direction = (index & 3) as usize;
                let (box_y, box_x) = level.coord((index / 4) as usize);

                let sok_y = box_y as isize + DELTA_Y[direction];
                let sok_x = box_x as isize + DELTA_X[direction];

                let (next_box_y, next_box_x, next_sok_y, next_sok_x);
                if pull_mode {
                    next_box_y = box_y as isize + DELTA_Y[direction];
                    next_box_x = box_x as isize + DELTA_X[direction];
                    next_sok_y = sok_y + DELTA_Y[direction];
                    next_sok_x = sok_x + DELTA_X[direction];
                    if b.get(next_sok_y, next_sok_x) & OCCUPIED != 0 {
                        continue;
                    }
                } else {
                    next_box_y = box_y as isize - DELTA_Y[direction];
                    next_box_x = box_x as isize - DELTA_X[direction];
                    next_sok_y = sok_y - DELTA_Y[direction];
                    next_sok_x = sok_x - DELTA_X[direction];
                    if b.get(next_box_y, next_box_x) & OCCUPIED != 0 {
                        continue;
                    }
                }

                if next_box_y < 0 || next_box_x < 0 {
                    continue;
                }
                let (next_box_y, next_box_x) = (next_box_y as usize, next_box_x as usize);
                if !b.in_bounds(next_box_y, next_box_x)
                    || self.current_impossible.cell(next_box_y, next_box_x) != 0
                {
                    continue;
                }
                let Some(next_box_index) = level.index_at(next_box_y, next_box_x) else {
                    continue;
                };
                let next_index = (next_box_index * 4 + direction) as u32;
                if self.vertices[next_index as usize].weight != INF {
                    continue;
                }

                self.vertices[next_index as usize].weight = d;
                self.vertices[next_index as usize].src = index as i32;
                next.push(next_index);
                let index_after_push = next_index;

                b.clear_sokoban_inplace();
                b.or(next_box_y, next_box_x, BOX);
                if next_sok_y >= 0 && next_sok_x >= 0 {
                    b.or(next_sok_y as usize, next_sok_x as usize, SOKOBAN);
                }
                b.expand_sokoban_cloud();
                self.update_shift(level, &b, next_box_y, next_box_x);

                for dir in 0..4 {
                    let sy = next_box_y as isize + DELTA_Y[dir];
                    let sx = next_box_x as isize + DELTA_X[dir];
                    if b.get(sy, sx) & SOKOBAN == 0 {
                        continue;
                    }
                    let next_index = (next_box_index * 4 + dir) as u32;
                    if self.vertices[next_index as usize].weight != INF {
                        continue;
                    }
                    self.vertices[next_index as usize].weight = d;
                    self.vertices[next_index as usize].src = index_after_push as i32;
                    next.push(next_index);
                }

                b.and_not(next_box_y, next_box_x, BOX);
            }

            std::mem::swap(&mut current, &mut next);
        }
    }

    fn can_move_from_index_to_index(&self, from: u32, to: u32) -> bool {
        debug_assert!((from ^ to) >> 2 == 0);
        self.vertices[from as usize].shift[(to & 3) as usize] == to
    }

    pub fn find_reversible_moves(
        &mut self,
        level: &Level,
        board: &Board,
        box_index: usize,
        pull_mode: bool,
    ) {
        let n = self.vertices_num;
        let mut visited = vec![false; n];
        let mut current: Vec<u32> = Vec::with_capacity(MAX_INNER);
        let mut next: Vec<u32> = Vec::with_capacity(MAX_INNER);

        let (box_y, box_x) = level.coord(box_index);
        debug_assert!(board.cell(box_y, box_x) & BOX != 0);
        let mut b = *board;
        b.and_not(box_y, box_x, BOX);

        for vertex in &mut self.vertices {
            vertex.reversible = false;
        }

        for dir in 0..4 {
            let y = box_y as isize + DELTA_Y[dir];
            let x = box_x as isize + DELTA_X[dir];
            if b.get(y, x) & SOKOBAN == 0 {
                continue;
            }
            let index = (box_index * 4 + dir) as u32;
            self.vertices[index as usize].reversible = true;
            current.push(index);
            visited[index as usize] = true;
        }

        while !current.is_empty() {
            next.clear();
            for &index in &current {
                for dir in 0..4 {
                    let next_index = (index & !3) + dir;
                    if visited[next_index as usize] {
                        continue;
                    }
                    if self.can_move_from_index_to_index(index, next_index)
                        && self.vertices[next_index as usize].src != -1
                    {
                        next.push(next_index);
                        self.vertices[next_index as usize].reversible = true;
                        visited[next_index as usize] = true;
                    }
                }

                let (mut by, mut bx) = level.coord((index >> 2) as usize);
                let direction = (index & 3) as usize;
                let mut sy = by as isize + DELTA_Y[direction];
                let mut sx = bx as isize + DELTA_X[direction];

                if b.get(sy, sx) & OCCUPIED != 0 {
                    debug_assert!(false, "back problem in find_reversible_moves");
                    continue;
                }

                if pull_mode {
                    let new_by = by as isize - DELTA_Y[direction];
                    let new_bx = bx as isize - DELTA_X[direction];
                    if new_by < 0 || new_bx < 0 || b.get(new_by, new_bx) & OCCUPIED != 0 {
                        continue;
                    }
                    by = new_by as usize;
                    bx = new_bx as usize;
                } else {
                    sy += DELTA_Y[direction];
                    sx += DELTA_X[direction];
                    let new_by = by as isize + DELTA_Y[direction];
                    let new_bx = bx as isize + DELTA_X[direction];
                    if new_by < 0 || new_bx < 0 || b.get(sy, sx) & OCCUPIED != 0 {
                        continue;
                    }
                    by = new_by as usize;
                    bx = new_bx as usize;
                }

                let Some(box_index) = level.index_at(by, bx) else {
                    continue;
                };
                let next_index = (box_index * 4 + direction) as u32;
                if self.current_impossible.cell(by, bx) != 0 || visited[next_index as usize] {
                    continue;
                }
                if self.vertices[next_index as usize].src != -1 {
                    next.push(next_index);
                    self.vertices[next_index as usize].reversible = true;
                    visited[next_index as usize] = true;
                }
            }
            std::mem::swap(&mut current, &mut next);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_current_impossible(
        &mut self,
        level: &Level,
        board_in: &Board,
        box_y: usize,
        box_x: usize,
        pull_mode: bool,
        possible_board: &Board,
        impossible_place: &[bool],
    ) {
        let b = *board_in;
        self.current_impossible.zero();

        for (i, impossible) in impossible_place.iter().enumerate().take(level.index_num()) {
            let (y, x) = level.coord(i);
            if *impossible {
                self.current_impossible.set(y, x, 1);
            }
            if b.cell(y, x) == WALL {
                self.current_impossible.set(y, x, 1);
            }
        }

        if !pull_mode {
            let mut without_box = b;
            without_box.and_not(box_y, box_x, BOX);
            let mut blocked = Board::new(b.width(), b.height()).expect("same dims");
            mark_deadlock_2x2(&without_box, &mut blocked);
            for y in 0..b.height() {
                for x in 0..b.width() {
                    if !level.is_inner(y, x) {
                        continue;
                    }
                    if blocked.cell(y, x) != 0 {
                        self.current_impossible.set(y, x, 1);
                    }
                }
            }
        }

        for y in 0..b.height() {
            for x in 0..b.width() {
                if !level.is_inner(y, x) {
                    continue;
                }
                if possible_board.cell(y, x) == 0 {
                    self.current_impossible.set(y, x, 1);
                }
            }
        }
    }

    pub fn make_holes_inactive(&mut self, level: &Level, board: &Board) {
        for y in 0..board.height() {
            for x in 0..board.width() {
                if !level.is_inner(y, x) || board.cell(y, x) & OCCUPIED != 0 {
                    continue;
                }
                if board.count_walls_around(y, x) != 3 {
                    continue;
                }
                for dir in 0..4 {
                    let box_y = y as isize + DELTA_Y[dir];
                    let box_x = x as isize + DELTA_X[dir];
                    if box_y < 0 || box_x < 0 {
                        continue;
                    }
                    let (box_y, box_x) = (box_y as usize, box_x as usize);
                    if !board.in_bounds(box_y, box_x)
                        || board.cell(box_y, box_x) == WALL
                        || !level.is_inner(box_y, box_x)
                    {
                        continue;
                    }
                    let Some(index) = level.index_at(box_y, box_x) else {
                        continue;
                    };
                    let slot = index * 4 + ((dir + 2) & 3);
                    if board.cell(y, x) & SOKOBAN != 0 && board.cell(box_y, box_x) & SOKOBAN != 0 {
                        self.vertices[slot].active = false;
                    }
                    if board.cell(y, x) & SOKOBAN == 0 {
                        self.vertices[slot].active = false;
                    }
                }
            }
        }
    }
}
