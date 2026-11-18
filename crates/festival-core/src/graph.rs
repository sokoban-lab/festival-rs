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
    shift_blocks: ShiftBlocks,
    visited_scratch: Vec<bool>,
    current_scratch: Vec<u32>,
    next_scratch: Vec<u32>,
    blocked_scratch: Board,
    static_impossible: Board,
    base_impossible: Board,
    tmp_board: Board,
    dirty_cells: [u16; 9],
    dirty_len: u8,
}

impl Graph {
    pub fn new(level: &Level) -> Self {
        let vertices_num = level.index_num() * 4;
        let width = level.initial.width();
        let height = level.initial.height();
        Self {
            vertices: vec![Vertex::default(); vertices_num],
            current_impossible: Board::new(width, height).expect("level dimensions"),
            vertices_num,
            shift_blocks: ShiftBlocks::default(),
            visited_scratch: Vec::with_capacity(vertices_num),
            current_scratch: Vec::with_capacity(vertices_num),
            next_scratch: Vec::with_capacity(vertices_num),
            blocked_scratch: Board::new(width, height).expect("level dimensions"),
            static_impossible: Board::new(width, height).expect("level dimensions"),
            base_impossible: Board::new(width, height).expect("level dimensions"),
            tmp_board: Board::new(width, height).expect("level dimensions"),
            dirty_cells: [0; 9],
            dirty_len: 0,
        }
    }

    /// Like `new`, but leaves the vertex array uninitialized; `build` fills it.
    pub fn unbuilt(level: &Level) -> Self {
        let vertices_num = level.index_num() * 4;
        let width = level.initial.width();
        let height = level.initial.height();
        Self {
            vertices: Vec::with_capacity(vertices_num),
            current_impossible: Board::new(width, height).expect("level dimensions"),
            vertices_num: 0,
            shift_blocks: ShiftBlocks::default(),
            visited_scratch: Vec::with_capacity(vertices_num),
            current_scratch: Vec::with_capacity(vertices_num),
            next_scratch: Vec::with_capacity(vertices_num),
            blocked_scratch: Board::new(width, height).expect("level dimensions"),
            static_impossible: Board::new(width, height).expect("level dimensions"),
            base_impossible: Board::new(width, height).expect("level dimensions"),
            tmp_board: Board::new(width, height).expect("level dimensions"),
            dirty_cells: [0; 9],
            dirty_len: 0,
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

        self.shift_blocks.compute(level, board);
        let edge_block = &self.shift_blocks.edge_block;

        for i in 0..level.index_num() {
            let (y, x) = level.coord(i);
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }

            for from in 0..4 {
                if !self.vertices[i * 4 + from].active {
                    continue;
                }
                let Some(from_index) = neighbor_index(level, board, i, from) else {
                    continue;
                };
                let from_edge = from_index * 4 + ((from + 2) & 3);
                for to in (from + 1)..4 {
                    if !self.vertices[i * 4 + to].active {
                        continue;
                    }
                    let Some(to_index) = neighbor_index(level, board, i, to) else {
                        continue;
                    };
                    let to_edge = to_index * 4 + ((to + 2) & 3);
                    // Two sides of a cell stay connected once the cell is
                    // occupied by a box exactly when both edges belong to the
                    // same biconnected block.
                    if edge_block[from_edge] == edge_block[to_edge] {
                        self.vertices[i * 4 + from].shift[to] = (i * 4 + to) as u32;
                        self.vertices[i * 4 + to].shift[from] = (i * 4 + from) as u32;
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
        self.visited_scratch.clear();
        self.visited_scratch.resize(n, false);
        self.current_scratch.clear();
        self.next_scratch.clear();

        let visited = &mut self.visited_scratch;
        let current = &mut self.current_scratch;
        let next = &mut self.next_scratch;

        for (i, vertex) in self.vertices.iter_mut().enumerate() {
            if vertex.weight == 0 {
                vertex.src = -1;
                next.push(i as u32);
                visited[i] = true;
            }
        }

        while !next.is_empty() {
            current.clear();
            std::mem::swap(current, next);

            for &v in current.iter() {
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

        bfs::init_dist(dist, board.width(), board.height());
        for i in 0..level.index_num() {
            let (y, x) = level.coord(i);
            dist[Board::flat_index(y, x)] = self.get_weight_around_cell(i);
        }
    }

    fn reset_for_box_moves(&mut self) {
        for vertex in &mut self.vertices {
            vertex.shift = [0; 4];
            vertex.weight = INF;
            vertex.src = -1;
            vertex.reversible = false;
        }
    }

    pub fn mark_box_moves_in_graph(
        &mut self,
        level: &Level,
        board_in: &Board,
        box_index: usize,
        pull_mode: bool,
    ) {
        let (box_y, box_x) = level.coord(box_index);
        debug_assert!(board_in.cell(box_y, box_x) & BOX != 0);
        let mut b = Board::new(board_in.width(), board_in.height()).expect("dims");
        b.copy_active_from(board_in);
        b.and_not(box_y, box_x, BOX);
        self.mark_box_moves_in_graph_without_box(level, &b, box_index, pull_mode);
    }

    /// Like `mark_box_moves_in_graph`, assuming `b` already has `BOX` cleared at `box_index`.
    pub fn mark_box_moves_in_graph_without_box(
        &mut self,
        level: &Level,
        b: &Board,
        box_index: usize,
        pull_mode: bool,
    ) {
        self.reset_for_box_moves();

        let (box_y, box_x) = level.coord(box_index);
        debug_assert!(b.cell(box_y, box_x) & BOX == 0);
        self.shift_blocks.compute(level, b);

        self.current_scratch.clear();
        self.next_scratch.clear();
        let current = &mut self.current_scratch;
        let next = &mut self.next_scratch;
        let edge_block = &self.shift_blocks.edge_block;

        for dir in 0..4 {
            let sy = box_y as isize + DELTA_Y[dir];
            let sx = box_x as isize + DELTA_X[dir];
            if b.get(sy, sx) & SOKOBAN != 0 {
                self.vertices[box_index * 4 + dir].weight = 0;
                current.push((box_index * 4 + dir) as u32);
            }
        }

        let mut d = 0u32;

        while !current.is_empty() {
            d += 1;
            next.clear();

            for &index in current.iter() {
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

                let mut has_sokoban = [false; 4];
                if b.cell(next_box_y, next_box_x) & OCCUPIED == 0
                    && b.get(next_sok_y, next_sok_x) & OCCUPIED == 0
                    && next_sok_y >= 0
                    && next_sok_x >= 0
                    && let Some(sok_idx) = level.index_at(next_sok_y as usize, next_sok_x as usize)
                {
                    let from_block = edge_block[sok_idx * 4 + ((direction + 2) & 3)];
                    for (dir, slot) in has_sokoban.iter_mut().enumerate() {
                        if dir == direction {
                            *slot = b.in_bounds(next_sok_y as usize, next_sok_x as usize);
                        } else if from_block != 0
                            && let Some(nbr) = neighbor_index(level, b, next_box_index, dir)
                        {
                            *slot = edge_block[nbr * 4 + ((dir + 2) & 3)] == from_block;
                        }
                    }
                } else {
                    self.tmp_board.copy_without_sokoban_from(b);
                    self.tmp_board.or(next_box_y, next_box_x, BOX);
                    if next_sok_y >= 0 && next_sok_x >= 0 {
                        self.tmp_board
                            .expand_sokoban_cloud_from(next_sok_y as usize, next_sok_x as usize);
                    }
                    for (dir, slot) in has_sokoban.iter_mut().enumerate() {
                        *slot = self.tmp_board.at_dir(next_box_y, next_box_x, dir) & SOKOBAN != 0;
                    }
                }

                let base = next_box_index * 4;
                for (from, &has_from) in has_sokoban.iter().enumerate() {
                    if !has_from {
                        continue;
                    }
                    for (to, &has_to) in has_sokoban.iter().enumerate() {
                        if has_to {
                            self.vertices[base + from].shift[to] = (base + to) as u32;
                        }
                    }
                }

                for (dir, &has_dir) in has_sokoban.iter().enumerate() {
                    if !has_dir {
                        continue;
                    }
                    let next_index = (base + dir) as u32;
                    if self.vertices[next_index as usize].weight != INF {
                        continue;
                    }
                    self.vertices[next_index as usize].weight = d;
                    self.vertices[next_index as usize].src = index_after_push as i32;
                    next.push(next_index);
                }
            }

            std::mem::swap(current, next);
        }
    }

    pub fn find_reversible_moves(
        &mut self,
        level: &Level,
        board: &Board,
        box_index: usize,
        pull_mode: bool,
    ) {
        let (box_y, box_x) = level.coord(box_index);
        debug_assert!(board.cell(box_y, box_x) & BOX != 0);
        for vertex in &mut self.vertices {
            vertex.reversible = false;
        }
        let mut b = Board::new(board.width(), board.height()).expect("dims");
        b.copy_active_from(board);
        b.and_not(box_y, box_x, BOX);
        self.find_reversible_moves_without_box(level, &b, box_index, pull_mode);
    }

    /// Like `find_reversible_moves`, assuming `b` already has `BOX` cleared at `box_index`
    /// and `vertices[*].reversible` is already `false` (from `mark_box_moves_in_graph_without_box`).
    pub fn find_reversible_moves_without_box(
        &mut self,
        level: &Level,
        b: &Board,
        box_index: usize,
        pull_mode: bool,
    ) {
        let n = self.vertices_num;
        self.visited_scratch.clear();
        self.visited_scratch.resize(n, false);
        self.current_scratch.clear();
        self.next_scratch.clear();

        let visited = &mut self.visited_scratch;
        let current = &mut self.current_scratch;
        let next = &mut self.next_scratch;

        let (box_y, box_x) = level.coord(box_index);
        debug_assert!(b.cell(box_y, box_x) & BOX == 0);

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
            for &index in current.iter() {
                for dir in 0..4u32 {
                    let next_index = (index & !3) + dir;
                    if visited[next_index as usize] {
                        continue;
                    }
                    if self.vertices[index as usize].shift[dir as usize] == next_index
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
            std::mem::swap(current, next);
        }
    }

    pub fn prepare_impossible_for_board(
        &mut self,
        level: &Level,
        board_in: &Board,
        pull_mode: bool,
        possible_board: &Board,
        impossible_place: &[bool],
    ) {
        self.dirty_len = 0;
        self.current_impossible.zero();

        let b_cells = board_in.cells();
        let possible_cells = possible_board.cells();

        if pull_mode {
            let out_cells = self.current_impossible.cells_mut();
            for (i, &(y, x)) in level.coords.iter().enumerate() {
                let flat = y as usize * MAX_SIZE + x as usize;
                if impossible_place.get(i).copied().unwrap_or(false)
                    || b_cells[flat] == WALL
                    || possible_cells[flat] == 0
                {
                    out_cells[flat] = 1;
                }
            }
            return;
        }

        self.static_impossible.zero();
        mark_deadlock_2x2(board_in, &mut self.blocked_scratch);

        let blocked_cells = self.blocked_scratch.cells();
        let static_cells = self.static_impossible.cells_mut();
        let out_cells = self.current_impossible.cells_mut();

        for (i, &(y, x)) in level.coords.iter().enumerate() {
            let flat = y as usize * MAX_SIZE + x as usize;
            let s_imp = impossible_place.get(i).copied().unwrap_or(false)
                || b_cells[flat] == WALL
                || possible_cells[flat] == 0;
            if s_imp {
                static_cells[flat] = 1;
            }
            if s_imp || blocked_cells[flat] != 0 {
                out_cells[flat] = 1;
            }
        }
        self.base_impossible
            .copy_active_from(&self.current_impossible);
    }

    pub fn update_current_impossible_for_removed_box(
        &mut self,
        level: &Level,
        board_without_box: &Board,
        box_y: usize,
        box_x: usize,
    ) {
        let base_cells = self.base_impossible.cells();
        let static_cells = self.static_impossible.cells();
        let out_cells = self.current_impossible.cells_mut();

        for &flat in &self.dirty_cells[..self.dirty_len as usize] {
            out_cells[flat as usize] = base_cells[flat as usize];
        }
        self.dirty_len = 0;

        let height = board_without_box.height();
        let width = board_without_box.width();
        if height < 2 || width < 2 {
            return;
        }

        let i_min = box_y.saturating_sub(2);
        let i_max = (box_y + 1).min(height - 2);
        let j_min = box_x.saturating_sub(2);
        let j_max = (box_x + 1).min(width - 2);

        let cells = board_without_box.cells();
        let mut local_blocked = [0u8; 25];

        for i in i_min..=i_max {
            let r0 = i * MAX_SIZE;
            let r1 = (i + 1) * MAX_SIZE;
            let ly = (i - i_min) * 5;
            for j in j_min..=j_max {
                let lx = j - j_min;
                let c00 = cells[r0 + j] & !SOKOBAN;
                let c01 = cells[r0 + j + 1] & !SOKOBAN;
                let c10 = cells[r1 + j] & !SOKOBAN;
                let c11 = cells[r1 + j + 1] & !SOKOBAN;

                let sum = u32::from(c00 & OCCUPIED != 0)
                    + u32::from(c01 & OCCUPIED != 0)
                    + u32::from(c10 & OCCUPIED != 0)
                    + u32::from(c11 & OCCUPIED != 0);
                let stuck = u32::from(c00 != WALL && c00 & TARGET == 0)
                    + u32::from(c01 != WALL && c01 & TARGET == 0)
                    + u32::from(c10 != WALL && c10 & TARGET == 0)
                    + u32::from(c11 != WALL && c11 & TARGET == 0);

                if sum == 3 && stuck != 0 {
                    local_blocked[ly + lx] = 1;
                    local_blocked[ly + lx + 1] = 1;
                    local_blocked[ly + 5 + lx] = 1;
                    local_blocked[ly + 5 + lx + 1] = 1;
                }

                if c00 == WALL && c11 == WALL {
                    if c01 == 0 {
                        local_blocked[ly + lx + 1] = 1;
                    }
                    if c10 == 0 {
                        local_blocked[ly + 5 + lx] = 1;
                    }
                }

                if c01 == WALL && c10 == WALL {
                    if c00 == 0 {
                        local_blocked[ly + lx] = 1;
                    }
                    if c11 == 0 {
                        local_blocked[ly + 5 + lx + 1] = 1;
                    }
                }
            }
        }

        let r_min = box_y.saturating_sub(1);
        let r_max = (box_y + 1).min(height - 1);
        let c_min = box_x.saturating_sub(1);
        let c_max = (box_x + 1).min(width - 1);

        for r in r_min..=r_max {
            let row = r * MAX_SIZE;
            let ly = (r - i_min) * 5;
            for c in c_min..=c_max {
                if level.index_at(r, c).is_none() {
                    continue;
                }
                let flat = row + c;
                let new_val =
                    u8::from(static_cells[flat] != 0 || local_blocked[ly + (c - j_min)] != 0);
                if out_cells[flat] != new_val {
                    out_cells[flat] = new_val;
                    self.dirty_cells[self.dirty_len as usize] = flat as u16;
                    self.dirty_len += 1;
                }
            }
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
        self.current_impossible.zero();

        if !pull_mode {
            let mut without_box = Board::new(board_in.width(), board_in.height()).expect("dims");
            without_box.copy_active_from(board_in);
            without_box.and_not(box_y, box_x, BOX);
            mark_deadlock_2x2(&without_box, &mut self.blocked_scratch);
        }

        let b_cells = board_in.cells();
        let possible_cells = possible_board.cells();
        let blocked_cells = self.blocked_scratch.cells();
        let out_cells = self.current_impossible.cells_mut();

        for (i, &(y, x)) in level.coords.iter().enumerate() {
            let flat = y as usize * MAX_SIZE + x as usize;
            let is_imp = impossible_place.get(i).copied().unwrap_or(false)
                || b_cells[flat] == WALL
                || possible_cells[flat] == 0
                || (!pull_mode && blocked_cells[flat] != 0);
            if is_imp {
                out_cells[flat] = 1;
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

const NO_PARENT: u16 = u16::MAX;

/// Edge-to-block labelling used by [`Graph::init_shift`].
///
/// A single Tarjan pass labels every directed edge of the free-cell graph with
/// the id of its biconnected block. Two sides of a cell stay connected once a
/// box occupies the cell exactly when both incident edges share a block, so the
/// per-cell component BFS is replaced by O(1) lookups.
#[derive(Debug, Default)]
struct ShiftBlocks {
    depth: Vec<u16>,
    low: Vec<u16>,
    parent: Vec<u16>,
    parent_dir: Vec<u8>,
    edge_block: Vec<u16>,
    edge_stack: Vec<u32>,
    dfs_stack: Vec<(u16, u8)>,
}

impl ShiftBlocks {
    fn compute(&mut self, level: &Level, board: &Board) {
        let n = level.index_num();
        self.depth.clear();
        self.depth.resize(n, 0);
        self.low.resize(n, 0);
        self.parent.resize(n, NO_PARENT);
        self.parent_dir.resize(n, 0);
        self.edge_block.clear();
        self.edge_block.resize(n * 4, 0);
        self.edge_stack.clear();
        self.dfs_stack.clear();

        let mut time = 0u16;
        let mut block = 0u16;

        for root in 0..n {
            let (root_y, root_x) = level.coord(root);
            if self.depth[root] != 0 || board.cell(root_y, root_x) & OCCUPIED != 0 {
                continue;
            }

            time += 1;
            self.depth[root] = time;
            self.low[root] = time;
            self.parent[root] = NO_PARENT;
            self.dfs_stack.push((root as u16, 0));

            while let Some(&(u, dir)) = self.dfs_stack.last() {
                let u = u as usize;
                if dir < 4 {
                    self.dfs_stack.last_mut().expect("frame").1 += 1;
                    let Some(v) = neighbor_index(level, board, u, dir as usize) else {
                        continue;
                    };
                    let (v_y, v_x) = level.coord(v);
                    if board.cell(v_y, v_x) & OCCUPIED != 0 {
                        continue;
                    }

                    if self.depth[v] == 0 {
                        time += 1;
                        self.depth[v] = time;
                        self.low[v] = time;
                        self.parent[v] = u as u16;
                        self.parent_dir[v] = dir;
                        self.edge_stack.push((u * 4 + dir as usize) as u32);
                        self.dfs_stack.push((v as u16, 0));
                    } else if self.parent[u] != v as u16 && self.depth[v] < self.depth[u] {
                        self.edge_stack.push((u * 4 + dir as usize) as u32);
                        if self.depth[v] < self.low[u] {
                            self.low[u] = self.depth[v];
                        }
                    }
                } else {
                    self.dfs_stack.pop();
                    let parent = self.parent[u];
                    if parent == NO_PARENT {
                        continue;
                    }
                    let parent = parent as usize;
                    if self.low[u] < self.low[parent] {
                        self.low[parent] = self.low[u];
                    }
                    if self.low[u] < self.depth[parent] {
                        continue;
                    }

                    block += 1;
                    let target = (parent * 4 + self.parent_dir[u] as usize) as u32;
                    loop {
                        let edge = self.edge_stack.pop().expect("edge stack underflow");
                        let edge_from = (edge / 4) as usize;
                        let edge_dir = (edge % 4) as usize;
                        self.edge_block[edge as usize] = block;
                        let edge_to = neighbor_index(level, board, edge_from, edge_dir)
                            .expect("edge endpoint is walkable");
                        self.edge_block[edge_to * 4 + ((edge_dir + 2) & 3)] = block;
                        if edge == target {
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn neighbor_index(level: &Level, board: &Board, index: usize, dir: usize) -> Option<usize> {
    let (y, x) = level.coord(index);
    let ny = y as isize + DELTA_Y[dir];
    let nx = x as isize + DELTA_X[dir];
    if ny < 0 || nx < 0 {
        return None;
    }
    let (ny, nx) = (ny as usize, nx as usize);
    if !board.in_bounds(ny, nx) {
        return None;
    }
    level.index_at(ny, nx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    fn connected_without(
        level: &Level,
        board: &Board,
        from: usize,
        to: usize,
        blocked: usize,
    ) -> bool {
        let mut seen = [false; MAX_INNER];
        let mut queue = vec![from];
        seen[from] = true;
        while let Some(u) = queue.pop() {
            if u == to {
                return true;
            }
            for dir in 0..4 {
                let Some(v) = neighbor_index(level, board, u, dir) else {
                    continue;
                };
                if v == blocked || seen[v] {
                    continue;
                }
                let (v_y, v_x) = level.coord(v);
                if board.cell(v_y, v_x) & OCCUPIED != 0 {
                    continue;
                }
                seen[v] = true;
                queue.push(v);
            }
        }
        false
    }

    fn is_walkable(level: &Level, board: &Board, index: usize) -> bool {
        let (y, x) = level.coord(index);
        board.cell(y, x) & OCCUPIED == 0
    }

    /// Brute-force reference: one BFS per side pair with the cell occupied.
    fn reference_shifts(level: &Level, board: &Board) -> Vec<[u32; 4]> {
        let n = level.index_num();
        let mut shifts = vec![[0u32; 4]; n * 4];
        for i in 0..n {
            let (y, x) = level.coord(i);
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }
            for from in 0..4 {
                let Some(from_index) = neighbor_index(level, board, i, from) else {
                    continue;
                };
                if !is_walkable(level, board, from_index) {
                    continue;
                }
                for to in (from + 1)..4 {
                    let Some(to_index) = neighbor_index(level, board, i, to) else {
                        continue;
                    };
                    if !is_walkable(level, board, to_index) {
                        continue;
                    }
                    if connected_without(level, board, from_index, to_index, i) {
                        shifts[i * 4 + from][to] = (i * 4 + to) as u32;
                        shifts[i * 4 + to][from] = (i * 4 + from) as u32;
                    }
                }
            }
        }
        shifts
    }

    fn graph_shifts(level: &Level, board: &Board) -> Vec<[u32; 4]> {
        let mut graph = Graph::new(level);
        graph.build(level, board, false);
        graph.vertices.iter().map(|vertex| vertex.shift).collect()
    }

    fn assert_shifts_match(level: &Level, board: &Board, context: &str) {
        let expected = reference_shifts(level, board);
        let actual = graph_shifts(level, board);
        for (i, (expected, actual)) in expected.iter().zip(&actual).enumerate() {
            assert_eq!(expected, actual, "{context}: cell {} dir {}", i / 4, i % 4);
        }
    }

    fn empty_board(text: &str) -> (Level, Board) {
        let level = level(text);
        let mut board = level.initial;
        board.clear_boxes_inplace();
        (level, board)
    }

    #[test]
    fn shift_matches_bruteforce_on_small_levels() {
        for text in [
            "#######\n#@ $ .#\n#######\n",
            "#########\n#@      #\n#  $    #\n#    .  #\n#########\n",
            "#######\n#@    #\n# # # #\n#  $  #\n#  .  #\n#######\n",
            "########\n#@     #\n#  ##  #\n# #  # #\n#  $ . #\n########\n",
            "#########\n#@  #   #\n#   #   #\n#  $#   #\n#   # . #\n#########\n",
            "##########\n#@   #   #\n#    #   #\n#  ###   #\n#        #\n#  $   . #\n##########\n",
        ] {
            let (level, board) = empty_board(text);
            assert_shifts_match(&level, &board, text);
        }
    }

    #[test]
    fn shift_matches_bruteforce_with_extra_walls() {
        let (level, base) = empty_board(
            "#############\n#@          #\n#           #\n#           #\n#     $     #\n#           #\n#           #\n#          .#\n#############\n",
        );
        let height = base.height();
        let width = base.width();
        let mut state = 0x1234_5678u32;
        for round in 0..300 {
            let mut board = base;
            for _ in 0..12 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let y = 1 + state as usize % (height - 2);
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let x = 1 + state as usize % (width - 2);
                board.set(y, x, WALL);
            }
            assert_shifts_match(&level, &board, &format!("round {round}"));
        }
    }

    fn reference_mark_box_moves(
        graph: &mut Graph,
        level: &Level,
        board_in: &Board,
        box_index: usize,
        pull_mode: bool,
    ) {
        let mut b = *board_in;
        graph.reset_for_box_moves();
        let (box_y, box_x) = level.coord(box_index);
        let mut current = Vec::new();
        let mut next = Vec::new();
        for dir in 0..4 {
            let sy = box_y as isize + DELTA_Y[dir];
            let sx = box_x as isize + DELTA_X[dir];
            if b.get(sy, sx) & SOKOBAN != 0 {
                graph.vertices[box_index * 4 + dir].weight = 0;
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
                let (by, bx) = level.coord((index / 4) as usize);
                let sok_y = by as isize + DELTA_Y[direction];
                let sok_x = bx as isize + DELTA_X[direction];
                let (next_by, next_bx, next_sy, next_sx) = if pull_mode {
                    let ns_y = sok_y + DELTA_Y[direction];
                    let ns_x = sok_x + DELTA_X[direction];
                    if b.get(ns_y, ns_x) & OCCUPIED != 0 {
                        continue;
                    }
                    (sok_y, sok_x, ns_y, ns_x)
                } else {
                    let nb_y = by as isize - DELTA_Y[direction];
                    let nb_x = bx as isize - DELTA_X[direction];
                    if b.get(nb_y, nb_x) & OCCUPIED != 0 {
                        continue;
                    }
                    (nb_y, nb_x, by as isize, bx as isize)
                };
                if next_by < 0 || next_bx < 0 {
                    continue;
                }
                let (next_by, next_bx) = (next_by as usize, next_bx as usize);
                if !b.in_bounds(next_by, next_bx)
                    || graph.current_impossible.cell(next_by, next_bx) != 0
                {
                    continue;
                }
                let Some(next_box_idx) = level.index_at(next_by, next_bx) else {
                    continue;
                };
                let next_index = (next_box_idx * 4 + direction) as u32;
                if graph.vertices[next_index as usize].weight != INF {
                    continue;
                }
                graph.vertices[next_index as usize].weight = d;
                graph.vertices[next_index as usize].src = index as i32;
                next.push(next_index);
                let after = next_index;

                b.clear_sokoban_inplace();
                b.or(next_by, next_bx, BOX);
                if next_sy >= 0 && next_sx >= 0 {
                    b.or(next_sy as usize, next_sx as usize, SOKOBAN);
                }
                b.expand_sokoban_cloud();
                let base = next_box_idx * 4;
                for from in 0..4 {
                    if b.at_dir(next_by, next_bx, from) & SOKOBAN == 0 {
                        continue;
                    }
                    for to in 0..4 {
                        if b.at_dir(next_by, next_bx, to) & SOKOBAN != 0 {
                            graph.vertices[base + from].shift[to] = (base + to) as u32;
                        }
                    }
                }
                for dir in 0..4 {
                    if b.at_dir(next_by, next_bx, dir) & SOKOBAN == 0 {
                        continue;
                    }
                    let ni = (base + dir) as u32;
                    if graph.vertices[ni as usize].weight != INF {
                        continue;
                    }
                    graph.vertices[ni as usize].weight = d;
                    graph.vertices[ni as usize].src = after as i32;
                    next.push(ni);
                }
                b.and_not(next_by, next_bx, BOX);
            }
            std::mem::swap(&mut current, &mut next);
        }
    }

    #[test]
    fn mark_box_moves_matches_floodfill_reference() {
        let level = level(
            "#############\n\
             #@    #     #\n\
             #  ##   ##  #\n\
             #     $     #\n\
             #  #  .  #  #\n\
             #     $     #\n\
             #  ##   ##  #\n\
             #     .     #\n\
             #############\n",
        );
        let mut g_fast = Graph::new(&level);
        let mut g_ref = Graph::new(&level);
        let mut possible = Board::new(level.board.width(), level.board.height()).unwrap();
        for &(y, x) in &level.coords {
            possible.set(y as usize, x as usize, 1);
        }
        let imp = vec![false; level.index_num()];

        let mut rng = 0xa5a5_5a5a_1234_5678u64;
        for _ in 0..200 {
            let mut b = level.board.without_boxes();
            b.clear_sokoban_inplace();
            let mut box_indices = Vec::new();
            for (idx, &(y, x)) in level.coords.iter().enumerate() {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                if (rng >> 61) == 0 {
                    b.or(y as usize, x as usize, BOX);
                    box_indices.push(idx);
                }
            }
            if box_indices.is_empty() {
                continue;
            }
            // Pick a free cell for sokoban and expand cloud
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start = (rng as usize) % level.index_num();
            let (sy, sx) = level.coord(start);
            if b.cell(sy, sx) & OCCUPIED != 0 {
                continue;
            }
            b.expand_sokoban_cloud_from(sy, sx);

            let chosen = box_indices[(rng as usize >> 8) % box_indices.len()];
            let (by, bx) = level.coord(chosen);

            for pull_mode in [false, true] {
                g_fast.prepare_impossible_for_board(&level, &b, pull_mode, &possible, &imp);
                for &bidx in &box_indices {
                    let (iy, ix) = level.coord(bidx);
                    if !pull_mode {
                        let mut wb = b;
                        wb.and_not(iy, ix, BOX);
                        g_fast.update_current_impossible_for_removed_box(&level, &wb, iy, ix);
                    }
                    g_ref.set_current_impossible(&level, &b, iy, ix, pull_mode, &possible, &imp);
                    assert_eq!(
                        g_fast.current_impossible, g_ref.current_impossible,
                        "impossible mismatch box={bidx} pull={pull_mode}"
                    );
                }

                g_fast.set_current_impossible(&level, &b, by, bx, pull_mode, &possible, &imp);
                g_ref.current_impossible = g_fast.current_impossible;

                g_fast.mark_box_moves_in_graph(&level, &b, chosen, pull_mode);
                reference_mark_box_moves(&mut g_ref, &level, &b, chosen, pull_mode);

                g_fast.find_reversible_moves(&level, &b, chosen, pull_mode);
                g_ref.find_reversible_moves(&level, &b, chosen, pull_mode);

                for v in 0..g_fast.vertices_num {
                    let vf = g_fast.vertices[v];
                    let vr = g_ref.vertices[v];
                    assert_eq!(
                        vf.weight, vr.weight,
                        "weight mismatch v={v} pull={pull_mode}"
                    );
                    assert_eq!(vf.src, vr.src, "src mismatch v={v} pull={pull_mode}");
                    assert_eq!(vf.shift, vr.shift, "shift mismatch v={v} pull={pull_mode}");
                    assert_eq!(
                        vf.reversible, vr.reversible,
                        "rev mismatch v={v} pull={pull_mode}"
                    );
                }
            }
        }
    }
}
