// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Arena-indexed FESS search tree (`tree.h` / `tree.cpp`).
//!
//! C++ stores raw pointers between nodes, expansions and boards. This port
//! keeps everything in vectors with stable indices, so the structure is
//! acyclic by construction.

use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::moves::{Move, MoveAttr, apply_move, attr_to_weight};
use crate::planning::imagine as imagine_planning;
use crate::search::advisors::{AdvisorsElement, is_an_advisor, set_advisors_inner};
use crate::search::helper::Helper;
use crate::search::hf;
use crate::search::max_dist::set_max_dist_data_in_node;
use crate::search::perimeter::Perimeter;
use crate::search::queue::IndexedHeap;
use crate::search::score::{ScoreElement, is_better_score, score_board};
use crate::solver::LevelContext;

pub type NodeId = u32;
pub type ExpansionId = u32;

pub const NONE_NODE: NodeId = u32::MAX;
pub const NONE_EXPANSION: ExpansionId = u32::MAX;

const NO_MOVE: i32 = -1;
const MAX_WEIGHT: i32 = 1_000_000;

#[derive(Clone, Copy, Debug, Default)]
pub struct NodeElement {
    pub hash: u64,
    pub score: ScoreElement,
    pub expansion: ExpansionId,
    pub deadlocked: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MoveHashData {
    pub move_data: Move,
    pub hash: u64,
    pub deadlocked: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ExpansionData {
    pub node: NodeId,
    pub board_offset: usize,
    pub father: ExpansionId,
    pub moves_num: u16,
    pub move_hash_place: usize,
    pub best_move: i32,
    pub best_weight: i32,
    pub best_score: ScoreElement,
    pub best_past: NodeId,
    pub label: i32,
    pub has_corral: bool,
    pub weight: i32,
    pub depth: u32,
    pub subtree_size: u32,
    pub next_son_to_check: u16,
    pub leaf_direction: i32,
    pub advisors: AdvisorsElement,
}

pub struct Tree {
    pub hash_array: Vec<NodeId>,
    pub mask: usize,
    pub nodes: Vec<NodeElement>,
    pub max_nodes: usize,
    pub expansions: Vec<ExpansionData>,
    pub max_expansions: usize,
    pub move_hashes: Vec<MoveHashData>,
    pub max_move_hashes: usize,
    pub boards: Vec<u8>,
    pub board_stride: usize,
    pub queues: Vec<IndexedHeap>,
    pub labels_num: usize,
    pub pull_mode: bool,
    pub search_mode: SearchMode,
    pub deadlock: crate::deadlock::state::DeadlockState,
    width: usize,
    height: usize,
}

impl Tree {
    pub fn new(log_max_nodes: u32, width: usize, height: usize) -> Result<Self> {
        if !(4..=30).contains(&log_max_nodes) || width == 0 || height == 0 {
            return Err(SolveError::Internal("bad tree geometry".into()));
        }

        let hash_size = 1usize << (log_max_nodes + 2);
        let max_nodes = 1usize << log_max_nodes;
        let max_expansions = 1usize << (log_max_nodes.saturating_sub(3));
        let max_move_hashes = 1usize << (log_max_nodes + 1);
        let board_stride = width * height;
        let boards_len = max_expansions
            .checked_mul(board_stride)
            .ok_or(SolveError::ResourceExhausted)?;

        Ok(Self {
            hash_array: vec![NONE_NODE; hash_size],
            mask: hash_size - 1,
            nodes: Vec::with_capacity(max_nodes),
            max_nodes,
            expansions: Vec::with_capacity(max_expansions),
            max_expansions,
            move_hashes: Vec::with_capacity(max_move_hashes),
            max_move_hashes,
            boards: vec![0; boards_len],
            board_stride,
            queues: Vec::new(),
            labels_num: 0,
            pull_mode: false,
            search_mode: SearchMode::Normal,
            deadlock: crate::deadlock::state::DeadlockState::default(),
            width,
            height,
        })
    }

    pub fn new_with_level(log_max_nodes: u32, level: &crate::level::Level) -> Result<Self> {
        Self::new(log_max_nodes, level.initial.width(), level.initial.height())
    }

    /// Builds the per-level deadlock tables (MPDB2, rooms) before searching.
    pub fn prepare_deadlock(&mut self, ctx: &LevelContext) -> Result<()> {
        self.deadlock = crate::deadlock::state::DeadlockState::prepare(ctx)?;
        Ok(())
    }

    pub fn reset(&mut self) {
        self.hash_array.fill(NONE_NODE);
        self.nodes.clear();
        self.expansions.clear();
        self.move_hashes.clear();
        for queue in &mut self.queues {
            queue.clear();
        }
        self.labels_num = 0;
    }

    pub fn tree_nearly_full(&self) -> bool {
        if self.expansions.len() > self.max_expansions.saturating_sub(2) {
            return true;
        }
        if self.nodes.len() > self.max_nodes.saturating_sub(MAX_MOVES) {
            return true;
        }
        if self.move_hashes.len() > self.max_move_hashes.saturating_sub(MAX_MOVES) {
            return true;
        }
        (self.expansions.len() + 1) * self.board_stride >= self.boards.len()
    }

    pub fn find_node_by_hash(&self, hash: u64) -> Option<NodeId> {
        let mut index = (hash as usize) & self.mask;
        loop {
            let place = self.hash_array[index];
            if place == NONE_NODE {
                return None;
            }
            if self.nodes[place as usize].hash == hash {
                return Some(place);
            }
            index = (index + 1) & self.mask;
        }
    }

    fn add_entry_to_hash_array(&mut self, hash: u64, node: NodeId) -> Result<()> {
        let mut index = (hash as usize) & self.mask;
        loop {
            let place = self.hash_array[index];
            if place == NONE_NODE {
                self.hash_array[index] = node;
                return Ok(());
            }
            if self.nodes[place as usize].hash == hash {
                return Err(SolveError::Internal("adding existing hash".into()));
            }
            index = (index + 1) & self.mask;
        }
    }

    fn add_node_to_tree(&mut self, hash: u64, score: ScoreElement) -> Result<NodeId> {
        if self.find_node_by_hash(hash).is_some() {
            return Err(SolveError::Internal("adding existing hash".into()));
        }
        if self.nodes.len() >= self.max_nodes {
            return Err(SolveError::ResourceExhausted);
        }

        let place = self.nodes.len() as NodeId;
        self.nodes.push(NodeElement {
            hash,
            score,
            expansion: NONE_EXPANSION,
            deadlocked: false,
        });
        self.add_entry_to_hash_array(hash, place)?;
        Ok(place)
    }

    pub fn board_is_in_tree(&self, board: &Board) -> bool {
        self.find_node_by_hash(board.hash()).is_some()
    }

    pub fn add_board_to_tree(
        &mut self,
        ctx: &LevelContext,
        helper: &Helper,
        board: &Board,
    ) -> Result<NodeId> {
        let score = score_board(ctx, helper, board, self.pull_mode, self.search_mode);
        self.add_node_to_tree(board.hash(), score)
    }

    fn board_at(&self, offset: usize) -> Result<Board> {
        let end = offset + self.board_stride;
        if end > self.boards.len() {
            return Err(SolveError::Internal("board offset out of range".into()));
        }
        Board::from_bytes(self.width, self.height, &self.boards[offset..end])
    }

    fn store_board(&mut self, offset: usize, board: &Board) -> Result<()> {
        let end = offset + self.board_stride;
        if end > self.boards.len() {
            return Err(SolveError::ResourceExhausted);
        }
        let mut pos = offset;
        for y in 0..self.height {
            for x in 0..self.width {
                self.boards[pos] = board.cell(y, x);
                pos += 1;
            }
        }
        Ok(())
    }

    fn push_expansion(&mut self, mut data: ExpansionData) -> Result<ExpansionId> {
        if self.expansions.len() >= self.max_expansions {
            return Err(SolveError::ResourceExhausted);
        }
        let id = self.expansions.len() as ExpansionId;
        data.board_offset = id as usize * self.board_stride;
        self.expansions.push(data);
        Ok(id)
    }

    pub fn set_root(
        &mut self,
        ctx: &LevelContext,
        board: Board,
        helper: &mut Helper,
    ) -> Result<ExpansionId> {
        let nodes_before_expansion = self.nodes.len();
        let hash = board.hash();
        let score = score_board(ctx, helper, &board, self.pull_mode, self.search_mode);

        let node = match self.find_node_by_hash(hash) {
            Some(node) => {
                if self.nodes[node as usize].expansion != NONE_EXPANSION {
                    return Err(SolveError::Internal("root is already in tree".into()));
                }
                node
            }
            None => self.add_node_to_tree(hash, score)?,
        };

        let id = self.expansions.len() as ExpansionId;
        self.nodes[node as usize].expansion = id;
        let expansion = ExpansionData {
            node,
            board_offset: 0,
            father: NONE_EXPANSION,
            moves_num: 0,
            move_hash_place: 0,
            best_move: NO_MOVE,
            best_weight: MAX_WEIGHT,
            best_score: ScoreElement::default(),
            best_past: node,
            label: -1,
            has_corral: false,
            weight: 0,
            depth: 0,
            subtree_size: 1,
            next_son_to_check: 0,
            leaf_direction: -1,
            advisors: AdvisorsElement::default(),
        };
        let id = self.push_expansion(expansion)?;
        self.store_board(self.expansions[id as usize].board_offset, &board)?;

        self.fill_expansion_structures(ctx, id, helper)?;
        self.set_deadlock_status(id, ctx);

        if self.search_mode == SearchMode::GirlSearch {
            self.block_existing_sons_tree(id, nodes_before_expansion, ctx)?;
        }

        Ok(id)
    }

    pub fn fill_expansion_structures(
        &mut self,
        ctx: &LevelContext,
        expansion: ExpansionId,
        helper: &mut Helper,
    ) -> Result<()> {
        let board_offset = self.expansions[expansion as usize].board_offset;
        let board = self.board_at(board_offset)?;

        let (moves, has_corral) = crate::moves::find_possible_moves(
            ctx,
            &board,
            self.pull_mode,
            self.search_mode,
            helper,
            &mut self.deadlock,
        )?;

        if moves.len() > MAX_MOVES || moves.len() > u16::MAX as usize {
            return Err(SolveError::TooManyMoves);
        }

        let move_hash_place = self.move_hashes.len();
        if move_hash_place + moves.len() > self.max_move_hashes {
            return Err(SolveError::ResourceExhausted);
        }

        for mv in moves {
            let mut child = board;
            apply_move(&mut child, &mv, self.search_mode, &ctx.level);
            let hash = child.hash();
            self.move_hashes.push(MoveHashData {
                move_data: mv,
                hash,
                deadlocked: false,
            });
            if self.find_node_by_hash(hash).is_none() {
                let score = score_board(ctx, helper, &child, self.pull_mode, self.search_mode);
                self.add_node_to_tree(hash, score)?;
            }
        }

        let moves_num = self.move_hashes.len() - move_hash_place;
        let e = &mut self.expansions[expansion as usize];
        e.moves_num = moves_num as u16;
        e.move_hash_place = move_hash_place;
        e.has_corral = has_corral;

        self.set_leaf_direction(expansion);
        Ok(())
    }

    fn set_leaf_direction(&mut self, expansion: ExpansionId) {
        let (node, start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.node, e.move_hash_place, e.moves_num as usize)
        };

        let mut leaf = -1;
        if !self.nodes[node as usize].deadlocked {
            for i in 0..count {
                let hash = self.move_hashes[start + i].hash;
                let Some(place) = self.find_node_by_hash(hash) else {
                    continue;
                };
                let child = self.nodes[place as usize];
                if child.deadlocked {
                    continue;
                }
                if child.expansion == NONE_EXPANSION {
                    leaf = i as i32;
                    break;
                }
            }
        }
        self.expansions[expansion as usize].leaf_direction = leaf;
    }

    pub fn expansion_board(&self, expansion: ExpansionId) -> Result<Board> {
        let offset = self.expansions[expansion as usize].board_offset;
        self.board_at(offset)
    }

    /// Port of `store_solution_in_helper`.
    pub fn store_solution(&self, expansion: ExpansionId, helper: &mut Helper) -> Result<()> {
        let depth = self.expansions[expansion as usize].depth as usize;
        if depth + 1 >= MAX_SOL_LEN {
            return Err(SolveError::Internal("solution is too long".into()));
        }

        let mut path = vec![NONE_EXPANSION; depth + 1];
        let mut current = Some(expansion);
        while let Some(id) = current {
            let node_depth = self.expansions[id as usize].depth as usize;
            if node_depth >= path.len() {
                return Err(SolveError::Internal("bad solution depth".into()));
            }
            path[node_depth] = id;
            current = match self.expansions[id as usize].father {
                NONE_EXPANSION => None,
                father => Some(father),
            };
        }

        if path[0] != 0 {
            return Err(SolveError::Internal(
                "solution does not start at root".into(),
            ));
        }

        helper.sol_moves.clear();
        for &id in path.iter().skip(1) {
            helper.sol_moves.push(self.get_move_to_node(id)?);
        }
        Ok(())
    }

    pub fn get_score_of_hash(&self, hash: u64) -> Option<ScoreElement> {
        self.find_node_by_hash(hash)
            .map(|place| self.nodes[place as usize].score)
    }

    pub fn node_is_solved(&self, expansion: ExpansionId, ctx: &LevelContext) -> bool {
        let e = &self.expansions[expansion as usize];
        if e.depth == 0 {
            return false;
        }
        match self.board_at(e.board_offset) {
            Ok(board) => board.is_solved(
                self.pull_mode,
                &ctx.level.initial,
                ctx.level.initial_sokoban,
            ),
            Err(_) => false,
        }
    }

    pub fn set_deadlock_status(&mut self, expansion: ExpansionId, ctx: &LevelContext) {
        let mut current = Some(expansion);
        while let Some(id) = current {
            let (node, start, count, father) = {
                let e = &self.expansions[id as usize];
                (e.node, e.move_hash_place, e.moves_num as usize, e.father)
            };

            if self.nodes[node as usize].deadlocked {
                break;
            }
            if self.node_is_solved(id, ctx) {
                break;
            }

            for i in 0..count {
                if self.move_hashes[start + i].deadlocked {
                    continue;
                }
                let hash = self.move_hashes[start + i].hash;
                if let Some(place) = self.find_node_by_hash(hash) {
                    if self.nodes[place as usize].deadlocked {
                        self.move_hashes[start + i].deadlocked = true;
                    }
                }
            }

            let all_deadlocked = (0..count).all(|i| self.move_hashes[start + i].deadlocked);
            if !all_deadlocked {
                break;
            }

            self.nodes[node as usize].deadlocked = true;
            current = if father == NONE_EXPANSION {
                None
            } else {
                Some(father)
            };
        }
    }

    pub fn set_best_move(&mut self, expansion: ExpansionId, ctx: &LevelContext) {
        let (node, start, count, weight) = {
            let e = &self.expansions[expansion as usize];
            (e.node, e.move_hash_place, e.moves_num as usize, e.weight)
        };

        self.expansions[expansion as usize].best_move = NO_MOVE;
        self.expansions[expansion as usize].best_weight = MAX_WEIGHT;

        if self.nodes[node as usize].deadlocked {
            return;
        }

        let mut best_weight = MAX_WEIGHT;
        let mut best_move = NO_MOVE;
        let mut best_score = ScoreElement::default();

        for i in 0..count {
            let move_hash = self.move_hashes[start + i];
            if move_hash.deadlocked {
                continue;
            }

            let Some(place) = self.find_node_by_hash(move_hash.hash) else {
                continue;
            };
            let child = self.nodes[place as usize];

            if child.expansion != NONE_EXPANSION {
                continue;
            }
            if child.deadlocked {
                self.move_hashes[start + i].deadlocked = true;
                continue;
            }

            let move_weight = move_hash.move_data.attr.weight as i32;
            if move_weight > best_weight {
                continue;
            }

            if move_weight < best_weight {
                best_weight = move_weight;
                best_move = i as i32;
                best_score = child.score;
                continue;
            }

            if self.search_mode == SearchMode::GirlSearch {
                continue;
            }

            if is_better_score(&child.score, &best_score, self.pull_mode, self.search_mode) {
                best_weight = move_weight;
                best_move = i as i32;
                best_score = child.score;
            }
        }

        if best_weight != MAX_WEIGHT {
            let e = &mut self.expansions[expansion as usize];
            e.best_move = best_move;
            e.best_score = best_score;
            e.best_weight = weight + best_weight;
        } else {
            self.set_deadlock_status(expansion, ctx);
        }
    }

    /// Port of `set_advisors_and_weights_to_last_node`: score hooks, advisors,
    /// move weights and the node's best move.
    pub fn set_advisors_and_weights(
        &mut self,
        expansion: ExpansionId,
        ctx: &LevelContext,
        helper: &mut Helper,
    ) -> Result<()> {
        if self.search_mode == SearchMode::NaiveSearch {
            crate::search::naive::set_naive_data_in_node(self, ctx, expansion)?;
        }
        set_max_dist_data_in_node(self, ctx, expansion)?;
        self.set_imagine_data_in_node(expansion, ctx, helper)?;

        let (node, start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.node, e.move_hash_place, e.moves_num as usize)
        };
        let board = self.expansion_board(expansion)?;
        let base_score = self.nodes[node as usize].score;

        let mut moves = Vec::with_capacity(count);
        let mut scores = Vec::with_capacity(count);
        let mut already_expanded = Vec::with_capacity(count);

        for i in 0..count {
            let move_hash = self.move_hashes[start + i];
            moves.push(move_hash.move_data);
            scores.push(self.get_score_of_hash(move_hash.hash).unwrap_or_default());
            already_expanded.push(
                self.find_node_by_hash(move_hash.hash)
                    .map(|place| self.nodes[place as usize].expansion != NONE_EXPANSION)
                    .unwrap_or(false),
            );
        }

        let advisors = set_advisors_inner(
            ctx,
            helper,
            &mut self.deadlock,
            &board,
            &base_score,
            &moves,
            &scores,
            self.pull_mode,
            self.search_mode,
            &already_expanded,
        )?;
        self.expansions[expansion as usize].advisors = advisors;

        for i in 0..count {
            let advisor = is_an_advisor(&advisors, i);
            let mut attr = MoveAttr {
                advisor,
                weight: self.move_hashes[start + i].move_data.attr.weight,
                reversible: self.move_hashes[start + i].move_data.attr.reversible,
            };
            attr.weight = attr_to_weight(&attr);
            self.move_hashes[start + i].move_data.attr = attr;
        }

        if !helper.weighted_search {
            for i in 0..count {
                self.move_hashes[start + i].move_data.attr.weight = 0;
            }
        }

        if count <= 2 && count > 0 {
            self.move_hashes[start].move_data.attr.weight = 0;
        }

        if helper.perimeter_found {
            for i in 0..count {
                self.move_hashes[start + i].move_data.attr.weight = 1;
            }
        }

        self.set_best_move(expansion, ctx);
        Ok(())
    }

    /// Port of `set_imagine_data_in_node`.
    fn set_imagine_data_in_node(
        &mut self,
        expansion: ExpansionId,
        ctx: &LevelContext,
        helper: &Helper,
    ) -> Result<()> {
        if self.search_mode == SearchMode::RevSearch && self.pull_mode {
            let (node, start, count) = {
                let e = &self.expansions[expansion as usize];
                (e.node, e.move_hash_place, e.moves_num as usize)
            };
            let board = self.expansion_board(expansion)?;
            let mut moves = Vec::with_capacity(count);
            let mut scores = Vec::with_capacity(count);
            for i in 0..count {
                moves.push(self.move_hashes[start + i].move_data);
                scores.push(
                    self.get_score_of_hash(self.move_hashes[start + i].hash)
                        .unwrap_or_default(),
                );
            }

            let mut base_score = self.nodes[node as usize].score;
            hf::set_rev_distance(ctx, helper, &board, &mut base_score, &moves, &mut scores);
            self.nodes[node as usize].score = base_score;
            for (i, score) in scores.iter().enumerate() {
                let hash = self.move_hashes[start + i].hash;
                let Some(place) = self.find_node_by_hash(hash) else {
                    continue;
                };
                self.nodes[place as usize].score.dist_to_imagined = score.dist_to_imagined;
            }
            return Ok(());
        }

        if self.pull_mode {
            return Ok(());
        }
        if matches!(
            self.search_mode,
            SearchMode::GirlSearch
                | SearchMode::NaiveSearch
                | SearchMode::RevSearch
                | SearchMode::ForwardWithBases
        ) {
            return Ok(());
        }

        let (node, start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.node, e.move_hash_place, e.moves_num as usize)
        };
        let board = self.expansion_board(expansion)?;

        let imagined = if self.search_mode == SearchMode::Normal {
            if helper.imagine.is_none() || helper.parking_order.len() != ctx.level.box_count {
                return Ok(());
            }
            imagine_planning::get_imagined_board(ctx, &board, helper)?.0
        } else {
            helper.imagined_hf_board
        };

        let mut moves = Vec::with_capacity(count);
        let mut scores = Vec::with_capacity(count);
        for i in 0..count {
            moves.push(self.move_hashes[start + i].move_data);
            scores.push(
                self.get_score_of_hash(self.move_hashes[start + i].hash)
                    .unwrap_or_default(),
            );
        }

        let mut base_score = self.nodes[node as usize].score;
        crate::analysis::match_distance::compute_imagine_distance(
            &ctx.level,
            &ctx.distances,
            &board,
            &imagined,
            &mut base_score,
            &moves,
            &mut scores,
            self.search_mode,
        );
        self.nodes[node as usize].score = base_score;

        for (i, score) in scores.iter().enumerate() {
            let hash = self.move_hashes[start + i].hash;
            let Some(place) = self.find_node_by_hash(hash) else {
                continue;
            };
            self.nodes[place as usize].score.dist_to_imagined = score.dist_to_imagined;
        }

        Ok(())
    }

    /// Marks a son as unusable without touching its node (`remove_son_from_tree`).
    pub fn remove_son_from_tree(&mut self, ctx: &LevelContext, expansion: ExpansionId, son: usize) {
        let index = self.expansions[expansion as usize].move_hash_place + son;
        self.move_hashes[index].deadlocked = true;
        self.set_deadlock_status(expansion, ctx);
    }

    fn best_move_is_available(&mut self, expansion: ExpansionId) -> bool {
        let (start, son) = {
            let e = &self.expansions[expansion as usize];
            (e.move_hash_place, e.best_move)
        };
        if son == NO_MOVE {
            return false;
        }
        let index = start + son as usize;
        if self.move_hashes[index].deadlocked {
            return false;
        }
        let Some(place) = self.find_node_by_hash(self.move_hashes[index].hash) else {
            return false;
        };
        let child = &mut self.nodes[place as usize];
        if child.deadlocked {
            self.move_hashes[index].deadlocked = true;
            return false;
        }
        if child.expansion != NONE_EXPANSION {
            return false;
        }
        true
    }

    /// Pops queue entries until an expansion with a playable best move is
    /// found. Returns `(weight, expansion, son)`, or weight 1_000_000.
    pub fn best_move_in_tree(&mut self, label: usize, ctx: &LevelContext) -> (i32, i32, i32) {
        if label >= self.queues.len() {
            return (MAX_WEIGHT, -1, -1);
        }

        loop {
            let expansions: &[ExpansionData] = &self.expansions;
            let popped = self.queues[label].pop_max(expansions, self.pull_mode, self.search_mode);
            let Some(position) = popped else {
                return (MAX_WEIGHT, -1, -1);
            };

            if self.expansions[position as usize].best_move == NO_MOVE {
                return (MAX_WEIGHT, -1, -1);
            }

            let mut result = None;
            if self.best_move_is_available(position) {
                let e = &self.expansions[position as usize];
                result = Some((e.best_weight, position as i32, e.best_move));
            }

            self.set_best_move(position, ctx);

            if self.expansions[position as usize].best_move != NO_MOVE {
                let expansions: &[ExpansionData] = &self.expansions;
                self.queues[label].insert(position, expansions, self.pull_mode, self.search_mode);
            }

            if let Some(result) = result {
                return result;
            }
        }
    }

    pub fn get_node_move(&self, expansion: ExpansionId, son: usize) -> Move {
        let e = &self.expansions[expansion as usize];
        self.move_hashes[e.move_hash_place + son].move_data
    }

    pub fn get_node_hash(&self, expansion: ExpansionId, son: usize) -> u64 {
        let e = &self.expansions[expansion as usize];
        self.move_hashes[e.move_hash_place + son].hash
    }

    pub fn expand_node(
        &mut self,
        ctx: &LevelContext,
        parent: ExpansionId,
        son: usize,
        helper: &mut Helper,
    ) -> Result<ExpansionId> {
        let nodes_before_expansion = self.nodes.len();
        let (parent_start, parent_depth, parent_weight, parent_best_past) = {
            let e = &self.expansions[parent as usize];
            (e.move_hash_place, e.depth, e.weight, e.best_past)
        };

        let move_hash = self.move_hashes[parent_start + son];
        let Some(node) = self.find_node_by_hash(move_hash.hash) else {
            return Err(SolveError::Internal("missing node".into()));
        };
        let node_element = self.nodes[node as usize];
        if node_element.expansion != NONE_EXPANSION {
            return Err(SolveError::Internal("already expanded".into()));
        }
        if node_element.deadlocked {
            return Err(SolveError::Internal(
                "expanding a known deadlocked node".into(),
            ));
        }

        let parent_board = self.board_at(self.expansions[parent as usize].board_offset)?;
        let mut board = parent_board;
        apply_move(
            &mut board,
            &move_hash.move_data,
            self.search_mode,
            &ctx.level,
        );
        if board.hash() != move_hash.hash {
            return Err(SolveError::Internal("hash error".into()));
        }

        let id = self.expansions.len() as ExpansionId;
        self.nodes[node as usize].expansion = id;

        let best_past = if is_better_score(
            &node_element.score,
            &self.nodes[parent_best_past as usize].score,
            self.pull_mode,
            self.search_mode,
        ) {
            node
        } else {
            parent_best_past
        };
        let expansion = ExpansionData {
            node,
            board_offset: 0,
            father: parent,
            moves_num: 0,
            move_hash_place: 0,
            best_move: NO_MOVE,
            best_weight: MAX_WEIGHT,
            best_score: ScoreElement::default(),
            best_past,
            label: -1,
            has_corral: false,
            weight: parent_weight + move_hash.move_data.attr.weight as i32,
            depth: parent_depth + 1,
            subtree_size: 0,
            next_son_to_check: 0,
            leaf_direction: -1,
            advisors: AdvisorsElement::default(),
        };
        let id = self.push_expansion(expansion)?;
        self.store_board(self.expansions[id as usize].board_offset, &board)?;

        self.fill_expansion_structures(ctx, id, helper)?;
        self.set_deadlock_status(id, ctx);
        self.update_subtree_size(id);

        if self.search_mode == SearchMode::GirlSearch {
            self.block_existing_sons_tree(id, nodes_before_expansion, ctx)?;
        }

        Ok(id)
    }

    /// Port of `block_existing_sons_tree`; only meaningful for GIRL search.
    pub fn block_existing_sons_tree(
        &mut self,
        expansion: ExpansionId,
        nodes_before_expansion: usize,
        ctx: &LevelContext,
    ) -> Result<()> {
        let (start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.move_hash_place, e.moves_num as usize)
        };

        for i in 0..count {
            let hash = self.move_hashes[start + i].hash;
            let place = self
                .find_node_by_hash(hash)
                .ok_or_else(|| SolveError::Internal("block_existing_sons: missing node".into()))?;
            if (place as usize) < nodes_before_expansion {
                self.move_hashes[start + i].deadlocked = true;
            }
        }

        self.set_deadlock_status(expansion, ctx);
        Ok(())
    }

    fn update_subtree_size(&mut self, expansion: ExpansionId) {
        let mut current = Some(expansion);
        while let Some(id) = current {
            let (node, father) = {
                let e = &self.expansions[id as usize];
                (e.node, e.father)
            };
            if self.nodes[node as usize].deadlocked {
                break;
            }
            self.expansions[id as usize].subtree_size += 1;
            current = if father == NONE_EXPANSION {
                None
            } else {
                Some(father)
            };
        }
    }

    pub fn set_node_as_deadlocked(
        &mut self,
        ctx: &LevelContext,
        expansion: ExpansionId,
        son: usize,
    ) -> Result<()> {
        let index = self.expansions[expansion as usize].move_hash_place + son;
        let Some(place) = self.find_node_by_hash(self.move_hashes[index].hash) else {
            return Err(SolveError::Internal("missing node".into()));
        };
        if self.nodes[place as usize].deadlocked {
            return Err(SolveError::Internal("already deadlocked".into()));
        }
        self.nodes[place as usize].deadlocked = true;
        self.move_hashes[index].deadlocked = true;
        self.set_deadlock_status(expansion, ctx);
        Ok(())
    }

    /// Port of `set_subtree_as_deadlocked` (`tree.cpp`).
    pub fn set_subtree_as_deadlocked(
        &mut self,
        ctx: &LevelContext,
        expansion: ExpansionId,
    ) -> Result<()> {
        let node = self.expansions[expansion as usize].node;
        if self.nodes[node as usize].deadlocked {
            return Ok(());
        }
        self.nodes[node as usize].deadlocked = true;

        let (start, count) = {
            let e = &self.expansions[expansion as usize];
            (e.move_hash_place, e.moves_num as usize)
        };
        for i in 0..count {
            self.move_hashes[start + i].deadlocked = true;
            let hash = self.move_hashes[start + i].hash;
            let place = self
                .find_node_by_hash(hash)
                .ok_or_else(|| SolveError::Internal("missing node".into()))?;
            if self.nodes[place as usize].expansion == NONE_EXPANSION {
                self.nodes[place as usize].deadlocked = true;
            } else {
                let child = self.nodes[place as usize].expansion;
                self.set_subtree_as_deadlocked(ctx, child)?;
            }
        }

        self.nodes[node as usize].deadlocked = false;
        self.set_deadlock_status(expansion, ctx);
        debug_assert!(self.nodes[node as usize].deadlocked, "subtree deadlock bug");
        Ok(())
    }

    /// Port of `learn_from_subtrees` (`stuck.cpp`).
    pub fn learn_from_subtrees(
        &mut self,
        ctx: &LevelContext,
        mut expansion: ExpansionId,
    ) -> Result<()> {
        let pull_mode = self.pull_mode;
        while expansion != NONE_EXPANSION {
            let e = self.expansions[expansion as usize];
            if e.subtree_size == 64 + 64 * pull_mode as u32 {
                let board = self.expansion_board(expansion)?;
                crate::deadlock::stuck::slow_xy_deadlock(
                    ctx,
                    &mut self.deadlock,
                    &board,
                    pull_mode,
                )?;
            }
            expansion = e.father;
        }
        Ok(())
    }

    /// Port of `recheck_tree_with_patterns` (`stuck.cpp`).
    pub fn recheck_tree_with_patterns(&mut self, ctx: &LevelContext) -> Result<()> {
        let mut matches = Vec::new();
        for i in 0..self.expansions.len() {
            let node = self.expansions[i].node;
            if self.nodes[node as usize].deadlocked {
                continue;
            }
            let board = self.expansion_board(i as u32)?;
            if crate::deadlock::stuck::tree_board_matches_patterns(
                ctx,
                &self.deadlock,
                &board,
                self.pull_mode,
            ) {
                matches.push(i as u32);
            }
        }
        for expansion in matches {
            self.set_subtree_as_deadlocked(ctx, expansion)?;
        }
        Ok(())
    }

    pub fn get_move_to_node(&self, expansion: ExpansionId) -> Result<Move> {
        let e = &self.expansions[expansion as usize];
        if e.father == NONE_EXPANSION {
            return Err(SolveError::Internal("no father".into()));
        }
        let father = &self.expansions[e.father as usize];
        let target_hash = self.nodes[e.node as usize].hash;
        for i in 0..father.moves_num as usize {
            let move_hash = &self.move_hashes[father.move_hash_place + i];
            if move_hash.hash == target_hash {
                return Ok(move_hash.move_data);
            }
        }
        Err(SolveError::Internal("missing move".into()))
    }

    pub fn add_label_to_last_node(
        &mut self,
        label: i32,
        ctx: &LevelContext,
        helper: &mut Helper,
    ) -> Result<()> {
        let last = self
            .expansions
            .len()
            .checked_sub(1)
            .ok_or_else(|| SolveError::Internal("no expansions".into()))?
            as ExpansionId;
        if !(0..MAX_TREE_LABELS as i32).contains(&label) {
            return Err(SolveError::Internal("label overflow".into()));
        }

        self.expansions[last as usize].label = label;

        let label_index = label as usize;
        if label_index >= self.labels_num {
            if label_index > self.labels_num {
                return Err(SolveError::Internal("label jumped".into()));
            }
            self.queues.push(IndexedHeap::new());
            self.labels_num += 1;
        }

        if self.search_mode == SearchMode::GirlSearch && !helper.perimeter_found {
            return Ok(());
        }

        if helper.perimeter_found {
            self.set_best_move(last, ctx);
        }

        if self.expansions[last as usize].best_move != NO_MOVE {
            let expansions: &[ExpansionData] = &self.expansions;
            self.queues[label_index].insert(last, expansions, self.pull_mode, self.search_mode);
        }
        Ok(())
    }

    pub fn insert_all_tree_nodes_to_queues(&mut self, ctx: &LevelContext) {
        for i in 0..self.expansions.len() {
            self.set_best_move(i as ExpansionId, ctx);
            let label = self.expansions[i].label;
            if self.expansions[i].best_move != NO_MOVE && label >= 0 {
                let expansions: &[ExpansionData] = &self.expansions;
                self.queues[label as usize].insert(
                    i as u32,
                    expansions,
                    self.pull_mode,
                    self.search_mode,
                );
            }
        }
    }

    pub fn replace_in_queue(&mut self, expansion: ExpansionId) {
        let label = self.expansions[expansion as usize].label;
        if label < 0 || label as usize >= self.queues.len() {
            return;
        }
        let label = label as usize;
        let has_move = self.expansions[expansion as usize].best_move != NO_MOVE;
        {
            let expansions: &[ExpansionData] = &self.expansions;
            self.queues[label].remove(expansion, expansions, self.pull_mode, self.search_mode);
        }
        if has_move {
            let expansions: &[ExpansionData] = &self.expansions;
            self.queues[label].insert(expansion, expansions, self.pull_mode, self.search_mode);
        }
    }

    pub fn last_expansion(&self) -> Option<ExpansionId> {
        self.expansions.len().checked_sub(1).map(|id| id as u32)
    }

    pub fn best_so_far_is_better(&self, current: ExpansionId, new: ExpansionId) -> bool {
        if self.search_mode == SearchMode::GirlSearch {
            return is_better_score(
                &self.nodes[self.expansions[new as usize].node as usize].score,
                &self.nodes[self.expansions[current as usize].node as usize].score,
                self.pull_mode,
                self.search_mode,
            );
        }

        let new_past = self.expansions[new as usize].best_past;
        let current_past = self.expansions[current as usize].best_past;
        is_better_score(
            &self.nodes[new_past as usize].score,
            &self.nodes[current_past as usize].score,
            self.pull_mode,
            self.search_mode,
        )
    }

    pub fn best_node_in_tree(&self) -> Option<ExpansionId> {
        let mut result: Option<usize> = None;

        for i in 0..self.expansions.len() {
            let e = &self.expansions[i];
            if self.nodes[e.node as usize].deadlocked {
                continue;
            }

            let Some(best) = result else {
                result = Some(i);
                continue;
            };
            let best_e = &self.expansions[best];

            if e.depth == 0 {
                continue;
            }
            if best_e.depth == 0 {
                result = Some(i);
                continue;
            }

            if e.has_corral {
                continue;
            }
            if best_e.has_corral {
                result = Some(i);
                continue;
            }

            if is_better_score(
                &self.nodes[e.node as usize].score,
                &self.nodes[best_e.node as usize].score,
                self.pull_mode,
                self.search_mode,
            ) {
                result = Some(i);
            }
        }

        result.map(|index| index as u32)
    }

    pub fn all_queues_empty(&self) -> bool {
        self.queues.iter().all(IndexedHeap::is_empty)
    }

    pub fn tree_has_unexpanded_nodes(&self) -> bool {
        for e in &self.expansions {
            if self.nodes[e.node as usize].deadlocked {
                continue;
            }
            for i in 0..e.moves_num as usize {
                let Some(place) =
                    self.find_node_by_hash(self.move_hashes[e.move_hash_place + i].hash)
                else {
                    continue;
                };
                let node = self.nodes[place as usize];
                if !node.deadlocked && node.expansion == NONE_EXPANSION {
                    return true;
                }
            }
        }
        false
    }

    /// Port of `check_if_perimeter_reached`. On a hit the last expansion gets
    /// a negative weight and the matching move weight `-1`.
    pub fn check_if_perimeter_reached(
        &mut self,
        perimeter: &Perimeter,
        ctx: &LevelContext,
        helper: &mut Helper,
    ) -> bool {
        let Some(last) = self.last_expansion() else {
            return false;
        };
        let (start, count, weight) = {
            let e = &self.expansions[last as usize];
            (e.move_hash_place, e.moves_num as usize, e.weight)
        };

        let e_depth = if weight < 0 { 2000 + weight } else { 0 };
        let opposite_side = u8::from(!self.pull_mode);
        let mut best_depth = i32::MAX;
        let mut best_son = -1i32;

        for i in 0..count {
            let hash = self.move_hashes[start + i].hash;
            let Some(depth) = perimeter.is_in(hash, opposite_side) else {
                continue;
            };

            if weight < 0 && depth as i32 >= e_depth {
                continue;
            }
            if self.move_hashes[start + i].deadlocked {
                continue;
            }

            let Some(place) = self.find_node_by_hash(hash) else {
                continue;
            };
            let node = self.nodes[place as usize];
            if node.deadlocked {
                continue;
            }
            if node.expansion != NONE_EXPANSION {
                continue;
            }

            if (depth as i32) < best_depth {
                best_depth = depth as i32;
                best_son = i as i32;
            }
        }

        if best_son == -1 {
            return false;
        }

        if !helper.perimeter_found && self.search_mode == SearchMode::GirlSearch {
            // GIRL descends by value until the frontier is met; from here on
            // the same tree is searched by FESS.
            self.insert_all_tree_nodes_to_queues(ctx);
        }

        self.expansions[last as usize].weight = -2000 + (best_depth + 1);
        self.move_hashes[start + best_son as usize]
            .move_data
            .attr
            .weight = -1;
        self.set_best_move(last, ctx);
        self.replace_in_queue(last);
        helper.perimeter_found = true;
        true
    }
}
