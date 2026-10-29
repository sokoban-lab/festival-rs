//! Envelope patterns (`envelope.cpp`).
//!
//! Envelopes are corral patterns that cannot be solved from any player zone.
//! They are learned once per level by the mini search and then used by the
//! fixed-box analysis to wallify unreachable box configurations.

use crate::bfs;
use crate::board::Board;
use crate::constants::*;
use crate::deadlock::mini::can_put_all_on_targets;
use crate::deadlock::state::DeadlockState;
use crate::error::{Result, SolveError};
use crate::level::Level;
use crate::solver::LevelContext;

pub const MAX_ENVELOPE_PATTERNS: usize = 50;

/// Pattern collection replacing the C++ globals `new_envelope_patterns` /
/// `new_envelope_patterns_num`.
#[derive(Clone, Debug, Default)]
pub struct EnvelopePatterns {
    patterns: Vec<Board>,
}

impl EnvelopePatterns {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.patterns.clear();
    }

    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    pub fn patterns(&self) -> &[Board] {
        &self.patterns
    }

    /// Returns `false` when the C++ 50-pattern cap is reached.
    pub fn add(&mut self, pattern: Board) -> bool {
        if self.patterns.len() >= MAX_ENVELOPE_PATTERNS {
            return false;
        }
        self.patterns.push(pattern);
        true
    }

    pub fn fix_boxes_using_envelopes(&self, board: &mut Board) {
        for pattern in &self.patterns {
            if matches_envelope_pattern(board, pattern) {
                wallify_boxes_using_pattern(board, pattern);
            }
        }
    }

    pub fn already_in_envelopes(&self, board: &Board) -> bool {
        self.patterns
            .iter()
            .any(|pattern| matches_envelope_pattern(board, pattern))
    }
}

/// All pattern boxes must be in place; a wall counts as good as a box.
pub fn matches_envelope_pattern(board: &Board, pattern: &Board) -> bool {
    debug_assert_eq!(board.width(), pattern.width());
    debug_assert_eq!(board.height(), pattern.height());

    for y in 0..pattern.height() {
        for x in 0..pattern.width() {
            if pattern.cell(y, x) & BOX != 0 && board.cell(y, x) & OCCUPIED == 0 {
                return false;
            }
        }
    }

    // The sokoban must stay outside the corral, i.e. every player cell must be
    // marked as allowed by the pattern.
    for y in 0..pattern.height() {
        for x in 0..pattern.width() {
            if pattern.cell(y, x) & SOKOBAN == 0 && board.cell(y, x) & SOKOBAN != 0 {
                return false;
            }
        }
    }
    true
}

pub fn wallify_boxes_using_pattern(board: &mut Board, pattern: &Board) {
    for y in 0..pattern.height() {
        for x in 0..pattern.width() {
            if pattern.cell(y, x) & BOX != 0 {
                board.set(y, x, WALL);
            }
        }
    }
}

pub fn size_of_corral(level: &Level, corrals: &[u32], corral_index: u32) -> usize {
    let mut sum = 0;
    for y in 0..level.initial.height() {
        for x in 0..level.initial.width() {
            if corrals[Board::flat_index(y, x)] == corral_index {
                sum += 1;
            }
        }
    }
    sum
}

fn index_at_signed(level: &Level, y: isize, x: isize) -> Option<usize> {
    if y < 0 || x < 0 {
        return None;
    }
    let (y, x) = (y as usize, x as usize);
    if y >= level.initial.height() || x >= level.initial.width() {
        return None;
    }
    level.index_at(y, x)
}

/// Collects the targets adjacent (regular) and diagonal (diag) to a corral.
pub fn get_envelope_boxes(
    level: &Level,
    corrals: &[u32],
    corral_index: u32,
) -> (Vec<usize>, Vec<usize>) {
    let mut board = level.initial;
    board.clear_boxes_inplace();

    let mut regular_boxes = Vec::new();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if corrals[Board::flat_index(y, x)] != corral_index {
                continue;
            }
            for dir in 0..4 {
                let yy = y as isize + DELTA_Y[dir];
                let xx = x as isize + DELTA_X[dir];
                if board.get(yy, xx) & TARGET != 0 {
                    if let Some(index) = index_at_signed(level, yy, xx) {
                        regular_boxes.push(index);
                    }
                    if yy >= 0 && xx >= 0 {
                        board.and_not(yy as usize, xx as usize, TARGET);
                    }
                }
            }
        }
    }

    let mut diag_boxes = Vec::new();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if corrals[Board::flat_index(y, x)] != corral_index {
                continue;
            }
            for dir in 0..8 {
                let yy = y as isize + DELTA_Y_8[dir];
                let xx = x as isize + DELTA_X_8[dir];
                if board.get(yy, xx) & TARGET != 0 {
                    if let Some(index) = index_at_signed(level, yy, xx) {
                        diag_boxes.push(index);
                    }
                    if yy >= 0 && xx >= 0 {
                        board.and_not(yy as usize, xx as usize, TARGET);
                    }
                }
            }
        }
    }

    (regular_boxes, diag_boxes)
}

/// Builds the corral position for one diagonal-box subset.
pub fn set_up_corral(
    level: &Level,
    regular_boxes: &[usize],
    diag_boxes: &[usize],
    diag_indices: u32,
    board: &mut Board,
) {
    *board = level.initial;
    board.clear_boxes_inplace();
    board.clear_sokoban_inplace();

    for &index in regular_boxes {
        let (y, x) = level.coord(index);
        board.or(y, x, BOX);
    }

    for (i, &index) in diag_boxes.iter().enumerate() {
        if diag_indices & (1 << i) != 0 {
            let (y, x) = level.coord(index);
            board.or(y, x, BOX);
        }
    }
}

pub fn put_sokoban_out_of_corral(
    level: &Level,
    board: &mut Board,
    corrals: &[u32],
    corral_index: u32,
) {
    board.clear_sokoban_inplace();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if !level.is_inner(y, x) {
                continue;
            }
            if corrals[Board::flat_index(y, x)] == corral_index {
                continue;
            }
            if board.cell(y, x) & OCCUPIED != 0 {
                continue;
            }
            board.or(y, x, SOKOBAN);
        }
    }
}

fn put_sokoban_in_zone(board: &mut Board, zones: &[u32; MAX_INNER], z: u32) {
    board.clear_sokoban_inplace();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if zones[Board::flat_index(y, x)] == z && board.cell(y, x) & OCCUPIED == 0 {
                board.or(y, x, SOKOBAN);
            }
        }
    }
    board.expand_sokoban_cloud();
}

/// Tries to pack every box from each player zone outside the corral.
fn can_solve_from_any_zone(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    corrals: &[u32; MAX_INNER],
    corral_index: u32,
    board_in: &Board,
) -> Result<bool> {
    let mut board = *board_in;
    let mut zones = [INF; MAX_INNER];
    let zones_num = bfs::mark_connectivities(&ctx.level, &board, &mut zones);

    let mut corral_zone = INF;
    for y in 0..board.height() {
        for x in 0..board.width() {
            if corrals[Board::flat_index(y, x)] == corral_index {
                corral_zone = zones[Board::flat_index(y, x)];
            }
        }
    }
    if corral_zone == INF {
        return Err(SolveError::Internal("no corral zone".into()));
    }

    for z in 0..zones_num as u32 {
        if z == corral_zone {
            continue;
        }
        put_sokoban_in_zone(&mut board, &zones, z);
        // C++ treats -1 (search aborted) as "might be solvable".
        if can_put_all_on_targets(ctx, state, &board, 100)? != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn analyse_envelope(
    ctx: &LevelContext,
    state: &mut DeadlockState,
    corrals: &[u32; MAX_INNER],
    corral_index: u32,
    regular_boxes: &[usize],
    diag_boxes: &[usize],
) -> Result<()> {
    if diag_boxes.len() >= 8 {
        return Ok(());
    }

    for enum_diag in 0..=diag_boxes.len() {
        for mask in 0..(1u32 << diag_boxes.len()) {
            if mask.count_ones() as usize != enum_diag {
                continue;
            }

            let mut board = ctx.level.initial;
            set_up_corral(&ctx.level, regular_boxes, diag_boxes, mask, &mut board);
            put_sokoban_out_of_corral(&ctx.level, &mut board, corrals, corral_index);

            if board.boxes_in_level() == ctx.level.box_count {
                continue;
            }
            if state.envelopes.already_in_envelopes(&board) {
                continue;
            }
            if can_solve_from_any_zone(ctx, state, corrals, corral_index, &board)? {
                continue;
            }

            if !state.envelopes.add(board) {
                continue;
            }
        }
    }
    Ok(())
}

/// `init_envelope_patterns`: learns the level's unsolvable corral patterns.
pub fn init_envelope_patterns(ctx: &LevelContext, state: &mut DeadlockState) -> Result<()> {
    state.envelopes.clear();

    let mut board = ctx.level.initial;
    board.enter_reverse_mode(&ctx.level.initial, false);

    let mut corrals = [INF; MAX_INNER];
    let zones = bfs::mark_connectivities(&ctx.level, &board, &mut corrals);
    if zones == 1 {
        return Ok(());
    }

    for index in 0..zones as u32 {
        let (regular_boxes, diag_boxes) = get_envelope_boxes(&ctx.level, &corrals, index);
        if size_of_corral(&ctx.level, &corrals, index) > ctx.level.index_num() / 2 {
            continue;
        }
        analyse_envelope(ctx, state, &corrals, index, &regular_boxes, &diag_boxes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn test_level() -> Level {
        Level::preprocess(parse_levels("#######\n#@ $ .#\n#######\n").remove(0)).unwrap()
    }

    #[test]
    fn wallifies_matching_pattern_boxes() {
        let level = test_level();
        let mut pattern = level.initial;
        pattern.zero();
        pattern.set(1, 3, BOX);
        pattern.set(1, 1, SOKOBAN);

        let mut board = level.initial;
        assert!(matches_envelope_pattern(&board, &pattern));

        let mut envelopes = EnvelopePatterns::new();
        assert!(envelopes.add(pattern));
        envelopes.fix_boxes_using_envelopes(&mut board);
        assert_eq!(board.cell(1, 3), WALL);
    }

    #[test]
    fn rejects_pattern_when_sokoban_is_inside() {
        let level = test_level();
        let mut pattern = level.initial;
        pattern.zero();
        pattern.set(1, 2, BOX);
        // Intentionally no SOKOBAN allowance around the actual player.
        assert!(!matches_envelope_pattern(&level.initial, &pattern));
    }

    #[test]
    fn collects_adjacent_and_diagonal_targets() {
        let level = Level::preprocess(
            parse_levels("#######\n#@ $ .#\n#  .  #\n#   . #\n#######\n").remove(0),
        )
        .unwrap();
        let mut corrals = vec![INF; MAX_INNER];
        corrals[Board::flat_index(1, 1)] = 0;
        corrals[Board::flat_index(1, 2)] = 0;

        let (regular, diag) = get_envelope_boxes(&level, &corrals, 0);
        assert!(!regular.is_empty() || !diag.is_empty());
    }
}
