//! Snail / netlock level detection and scoring (`snail.cpp`).
//!
//! Both modes replace the regular score payload when the level's targets form
//! a long pull chain. Detection runs once per level, like `preprocess_level`.

use crate::board::Board;
use crate::constants::*;
use crate::level::Level;
use crate::search::helper::Helper;

#[derive(Debug)]
pub struct SnailParams {
    pub snail_level_detected: bool,
    pub netlock_level_detected: bool,
    pub target: (usize, usize),
    /// Distance from the chain's peel end; `INF` outside the chain.
    ///
    /// C++ stores this in an `int` board and can produce `-1` when the
    /// detected target is not the chain's far end; keep the signed value.
    pub chain_dist: [i32; MAX_INNER],
    pub chain: [bool; MAX_INNER],
    pub chain_len: usize,
    pub netlock_grid: [bool; MAX_INNER],
}

impl SnailParams {
    pub fn detect(level: &Level) -> Self {
        let mut params = Self {
            snail_level_detected: false,
            netlock_level_detected: false,
            target: (0, 0),
            chain_dist: [INF as i32; MAX_INNER],
            chain: [false; MAX_INNER],
            chain_len: 0,
            netlock_grid: [false; MAX_INNER],
        };

        let mut board = level.board;
        board.enter_reverse_mode(&level.initial, false);
        for y in 0..board.height() {
            for x in 0..board.width() {
                if level.initial.cell(y, x) & BOX == 0 {
                    board.and_not(y, x, BOX);
                }
            }
        }

        let mut max_difficulty = -1i32;
        for y in 0..board.height() {
            for x in 0..board.width() {
                if board.cell(y, x) & TARGET == 0 {
                    continue;
                }
                if level.initial.cell(y, x) & BOX != 0 {
                    continue;
                }

                board.or(y, x, BOX);
                let order = compute_pull_order(&board);
                board.and_not(y, x, BOX);

                let difficulty = order[Board::flat_index(y, x)];
                if difficulty > max_difficulty {
                    max_difficulty = difficulty;
                    params.target = (y, x);
                }
            }
        }

        params.set_netlock_grid(level);
        debug_assert!(max_difficulty >= -1);

        if max_difficulty >= 20 {
            params.snail_level_detected = true;
            params.analyze_chain(&board);
        } else if max_difficulty >= 10 {
            params.netlock_level_detected = true;
        }
        params
    }

    fn set_netlock_grid(&mut self, level: &Level) {
        let (ty, tx) = self.target;
        for y in 0..level.board.height() {
            for x in 0..level.board.width() {
                if !level.inner_at(y, x) {
                    continue;
                }
                if ((y ^ ty) & 1) == 0 && ((x ^ tx) & 1) == 0 {
                    self.netlock_grid[Board::flat_index(y, x)] = true;
                }
            }
        }
    }

    fn analyze_chain(&mut self, board: &Board) {
        let not_targets = board_without_targets(board);
        let mut dist = [INF; MAX_INNER];
        let (ty, tx) = self.target;
        crate::bfs::bfs_from_place(&not_targets, &mut dist, ty, tx, true);

        let mut chain_len = -1i32;
        let mut peel = (ty, tx);
        for y in 0..board.height() {
            for x in 0..board.width() {
                let value = dist[Board::flat_index(y, x)];
                if value == INF {
                    continue;
                }
                self.chain[Board::flat_index(y, x)] = true;
                if value as i32 > chain_len {
                    chain_len = value as i32;
                    peel = (y, x);
                }
            }
        }

        let mut dist = [INF; MAX_INNER];
        crate::bfs::bfs_from_place(&not_targets, &mut dist, peel.0, peel.1, true);
        let mut chain_dist = [INF as i32; MAX_INNER];
        for y in 0..board.height() {
            for x in 0..board.width() {
                let index = Board::flat_index(y, x);
                if dist[index] != INF {
                    chain_dist[index] = chain_len - dist[index] as i32;
                }
            }
        }

        self.chain_dist = chain_dist;
        self.chain_len = (chain_len + 1) as usize;
    }

    /// `snail_packed`: number of chain targets packed from the peel end.
    fn snail_packed(&self, board: &Board) -> i16 {
        let mut seq = vec![true; self.chain_len];
        for y in 0..board.height() {
            for x in 0..board.width() {
                let index = Board::flat_index(y, x);
                if !self.chain[index] {
                    continue;
                }
                if board.cell(y, x) & BOX == 0 {
                    let distance = self.chain_dist[index];
                    // C++ indexes `seq` with the raw value; a negative slot is
                    // out-of-bounds there and is skipped here.
                    if (0..seq.len() as i32).contains(&distance) {
                        seq[distance as usize] = false;
                    }
                }
            }
        }

        for (i, packed) in seq.iter().enumerate().skip(1) {
            if !packed {
                return i as i16;
            }
        }
        // C++ returns the loop counter, which stays at 1 for an empty chain.
        self.chain_len.max(1) as i16
    }
}

fn board_without_targets(board: &Board) -> Board {
    let mut result = *board;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & TARGET != 0 {
                result.set(y, x, 0);
            } else {
                result.set(y, x, WALL);
            }
        }
    }
    result
}

fn simple_box_can_be_pulled(board: &Board, y: usize, x: usize) -> bool {
    for dir in 0..4 {
        let dy = DELTA_Y[dir];
        let dx = DELTA_X[dir];
        if !board.is_occupied(y as isize + dy, x as isize + dx)
            && !board.is_occupied(y as isize + 2 * dy, x as isize + 2 * dx)
        {
            return true;
        }
    }
    false
}

/// `compute_pull_order`: layer at which each box can be pulled off the solved
/// position; unreachable boxes keep `-1`.
pub fn compute_pull_order(board_in: &Board) -> [i32; MAX_INNER] {
    let mut board = *board_in;
    let mut visited = [false; MAX_INNER];
    let mut order = [-1i32; MAX_INNER];

    let mut current: Vec<usize> = Vec::new();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            if simple_box_can_be_pulled(&board, y, x) {
                let index = Board::flat_index(y, x);
                visited[index] = true;
                current.push(index);
            }
        }
    }

    let mut depth = 0i32;
    while !current.is_empty() {
        for &index in &current {
            let y = index / MAX_SIZE;
            let x = index % MAX_SIZE;
            order[index] = depth;
            board.and_not(y, x, BOX);
        }

        depth += 1;
        let mut next = Vec::new();

        for &index in &current {
            let y = index / MAX_SIZE;
            let x = index % MAX_SIZE;

            for dir in 0..4 {
                let dy = DELTA_Y[dir];
                let dx = DELTA_X[dir];
                let box_y = y as isize + dy;
                let box_x = x as isize + dx;
                let sok_y = y as isize - dy;
                let sok_x = x as isize - dx;

                if board.get(box_y, box_x) & BOX == 0 {
                    continue;
                }
                let box_index = Board::flat_index(box_y as usize, box_x as usize);
                if visited[box_index] {
                    continue;
                }
                if board.is_occupied(sok_y, sok_x) {
                    continue;
                }
                visited[box_index] = true;
                next.push(box_index);
            }

            for dir in 0..4 {
                let dy = DELTA_Y[dir];
                let dx = DELTA_X[dir];
                if board.is_occupied(y as isize + dy, x as isize + dx) {
                    continue;
                }
                let box_y = y as isize + 2 * dy;
                let box_x = x as isize + 2 * dx;
                if board.get(box_y, box_x) & BOX == 0 {
                    continue;
                }
                let box_index = Board::flat_index(box_y as usize, box_x as usize);
                if visited[box_index] {
                    continue;
                }
                visited[box_index] = true;
                next.push(box_index);
            }
        }

        current = next;
    }

    order
}

fn touched_boxes(board: &Board) -> i32 {
    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            for dir in 0..4 {
                if board.get(y as isize + DELTA_Y[dir], x as isize + DELTA_X[dir]) & SOKOBAN != 0 {
                    sum += 1;
                    break;
                }
            }
        }
    }
    sum
}

fn count_101(board: &Board) -> i16 {
    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            if !board.is_occupied(y as isize, x as isize + 1)
                && board.get(y as isize, x as isize + 2) & BOX != 0
            {
                sum += 1;
            }
            if !board.is_occupied(y as isize + 1, x as isize)
                && board.get(y as isize + 2, x as isize) & BOX != 0
            {
                sum += 1;
            }
        }
    }
    sum
}

fn unlikely_pattern(board: &Board) -> i32 {
    let mut result = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX == 0 {
                continue;
            }
            let mut sum = 0;
            for dir in 0..8 {
                if board.get(y as isize + DELTA_Y_8[dir], x as isize + DELTA_X_8[dir]) & OCCUPIED
                    != 0
                {
                    sum += 1;
                }
            }
            if sum >= 6 {
                result += 1;
            }
        }
    }
    result
}

fn pullable_boxes(board: &Board) -> i16 {
    let mut result = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if board.cell(y, x) & BOX != 0 && simple_box_can_be_pulled(board, y, x) {
                result += 1;
            }
        }
    }
    result
}

fn boxes_on_grid(params: &SnailParams, board: &Board) -> i16 {
    let mut sum = 0;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if params.netlock_grid[Board::flat_index(y, x)] && board.cell(y, x) & BOX != 0 {
                sum += 1;
            }
        }
    }
    sum
}

fn netlock_score(params: &SnailParams, board: &Board) -> i16 {
    let (ty, tx) = params.target;
    if board.get(ty as isize, tx as isize) & BOX == 0 {
        return 0;
    }

    let order = compute_pull_order(board);
    let result = order[Board::flat_index(ty, tx)];
    if result == -1 { 1000 } else { result as i16 }
}

/// `fill_snail_scores`.
pub fn fill_snail_scores(
    params: &SnailParams,
    level: &Level,
    board: &Board,
    score: &mut crate::search::ScoreElement,
    alg_type: i32,
) {
    let (ty, tx) = params.target;
    score.out_of_plan = i16::from(board.get(ty as isize, tx as isize) & BOX != 0);

    if alg_type == 0 {
        score.dist_to_imagined = unlikely_pattern(board);
    }
    score.biconnect = level.box_count as i16 - touched_boxes(board) as i16;

    if score.out_of_plan == 1 {
        if alg_type == 0 {
            score.hotspots = count_101(board);
        }
    } else {
        score.boxes_on_targets = params.snail_packed(board);
    }
}

/// `is_better_snail_score`.
pub fn is_better_snail_score(
    new_score: &crate::search::ScoreElement,
    old_score: &crate::search::ScoreElement,
    _pull_mode: bool,
) -> bool {
    if new_score.out_of_plan < old_score.out_of_plan {
        return true;
    }
    if new_score.out_of_plan > old_score.out_of_plan {
        return false;
    }
    if new_score.biconnect < old_score.biconnect {
        return true;
    }
    if new_score.biconnect > old_score.biconnect {
        return false;
    }
    if new_score.dist_to_imagined < old_score.dist_to_imagined {
        return true;
    }
    if new_score.dist_to_imagined > old_score.dist_to_imagined {
        return false;
    }

    if new_score.out_of_plan == 0 {
        if new_score.boxes_on_targets > old_score.boxes_on_targets {
            return true;
        }
        if new_score.boxes_on_targets < old_score.boxes_on_targets {
            return false;
        }
    } else {
        if new_score.hotspots > old_score.hotspots {
            return true;
        }
        if new_score.hotspots < old_score.hotspots {
            return false;
        }
    }

    if new_score.dist_to_targets > old_score.dist_to_targets {
        return true;
    }
    if new_score.dist_to_targets < old_score.dist_to_targets {
        return false;
    }
    false
}

/// `fill_netlock_scores`.
pub fn fill_netlock_scores(
    params: &SnailParams,
    level: &Level,
    board: &Board,
    score: &mut crate::search::ScoreElement,
    alg_type: i32,
) {
    score.connectivity = crate::bfs::get_connectivity(level, board) as i16;
    score.boxes_on_targets = netlock_score(params, board);
    score.rooms_score = boxes_on_grid(params, board);

    if score.boxes_on_targets == 0 {
        score.imagine = pullable_boxes(board);
    } else {
        if alg_type == 0 {
            score.rooms_score = count_101(board);
            score.imagine = boxes_on_grid(params, board);
        }
        if alg_type == 2 {
            score.imagine = pullable_boxes(board);
        }
    }

    if alg_type == 3 {
        score.rooms_score = board.boxes_on_targets() as i16;
        score.imagine = count_101(board);
    }
}

/// `is_better_netlock_score`.
pub fn is_better_netlock_score(
    new_score: &crate::search::ScoreElement,
    old_score: &crate::search::ScoreElement,
) -> bool {
    if new_score.connectivity < old_score.connectivity {
        return true;
    }
    if new_score.connectivity > old_score.connectivity {
        return false;
    }
    if new_score.boxes_on_targets < old_score.boxes_on_targets {
        return true;
    }
    if new_score.boxes_on_targets > old_score.boxes_on_targets {
        return false;
    }
    if new_score.rooms_score > old_score.rooms_score {
        return true;
    }
    if new_score.rooms_score < old_score.rooms_score {
        return false;
    }
    if new_score.imagine > old_score.imagine {
        return true;
    }
    if new_score.imagine < old_score.imagine {
        return false;
    }
    false
}

/// `set_snail_parameters`.
pub fn set_snail_parameters(
    search_type: SearchMode,
    pull_mode: bool,
    params: &SnailParams,
    helper: &mut Helper,
) -> SearchMode {
    if !params.snail_level_detected {
        return search_type;
    }

    if !pull_mode {
        if matches!(
            search_type,
            SearchMode::ForwardWithBases | SearchMode::HfSearch | SearchMode::GirlSearch
        ) {
            return SearchMode::HfSearch;
        }
        return search_type;
    }

    match search_type {
        SearchMode::BaseSearch => {
            helper.alg_type = 0;
            helper.weighted_search = false;
            SearchMode::SnailSearch
        }
        SearchMode::MaxDistSearch2 => {
            helper.alg_type = 1;
            helper.weighted_search = false;
            SearchMode::SnailSearch
        }
        SearchMode::GirlSearch => {
            helper.alg_type = 3;
            helper.weighted_search = true;
            SearchMode::NetlockSearch
        }
        _ => search_type,
    }
}

/// `set_netlock_parameters`.
pub fn set_netlock_parameters(
    search_type: SearchMode,
    pull_mode: bool,
    params: &SnailParams,
    helper: &mut Helper,
) -> SearchMode {
    if !params.netlock_level_detected {
        return search_type;
    }

    if !pull_mode {
        if matches!(
            search_type,
            SearchMode::ForwardWithBases | SearchMode::HfSearch | SearchMode::GirlSearch
        ) {
            return SearchMode::HfSearch;
        }
        return search_type;
    }

    match search_type {
        SearchMode::BaseSearch => {
            helper.alg_type = 0;
            SearchMode::NetlockSearch
        }
        SearchMode::MaxDistSearch2 => {
            helper.alg_type = 1;
            SearchMode::NetlockSearch
        }
        SearchMode::GirlSearch => {
            helper.alg_type = 2;
            SearchMode::NetlockSearch
        }
        _ => search_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    #[test]
    fn pull_order_prefers_open_boxes() {
        let level =
            Level::preprocess(parse_levels("#######\n#@ $ .#\n#######\n").remove(0)).unwrap();
        let mut board = level.board;
        board.enter_reverse_mode(&level.initial, false);
        let order = compute_pull_order(&board);
        assert!(order.iter().any(|value| *value >= 0));
    }

    #[test]
    fn detection_is_off_for_tiny_levels() {
        let level =
            Level::preprocess(parse_levels("#######\n#@ $ .#\n#######\n").remove(0)).unwrap();
        let params = SnailParams::detect(&level);
        assert!(!params.snail_level_detected);
        assert!(!params.netlock_level_detected);
    }
}
