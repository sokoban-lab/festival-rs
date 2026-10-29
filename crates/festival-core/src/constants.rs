//! Numeric constants shared with the C++ implementation.
//!
//! Tile flags and limits are kept identical on purpose: differential testing
//! against `refcheck` compares raw board bytes and hashes.

pub const WALL: u8 = 1;
pub const BOX: u8 = 2;
pub const TARGET: u8 = 4;
pub const SOKOBAN: u8 = 8;
pub const BASE: u8 = 16;
pub const DEADLOCK_ZONE: u8 = 32;

pub const SPACE: u8 = 0;

pub const OCCUPIED: u8 = WALL | BOX;
pub const PACKED_BOX: u8 = BOX | TARGET;
pub const SOKOBAN_ON_TARGET: u8 = SOKOBAN | TARGET;

pub const MAX_SIZE: usize = 51;
pub const MAX_INNER: usize = MAX_SIZE * MAX_SIZE;
pub const MAX_BOXES: usize = 200;
pub const MAX_SOL_LEN: usize = 1500;
pub const MAX_MOVES: usize = 11_500;
pub const MAX_ROOMS: usize = 70;
pub const MAX_REQUESTS: usize = 10_000;
pub const MAX_TREE_LABELS: usize = 10_000;

/// C++ uses `1000000` both for "unreachable distance" and unassigned zones.
pub const INF: u32 = 1_000_000;

pub const DELTA_Y: [isize; 4] = [-1, 0, 1, 0];
pub const DELTA_X: [isize; 4] = [0, 1, 0, -1];

pub const DELTA_Y_8: [isize; 8] = [-1, -1, 0, 1, 1, 1, 0, -1];
pub const DELTA_X_8: [isize; 8] = [0, 1, 1, 1, 0, -1, -1, -1];

/// Direction indices follow the C++ order: up, right, down, left.
pub const UP: usize = 0;
pub const RIGHT: usize = 1;
pub const DOWN: usize = 2;
pub const LEFT: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SearchMode {
    Normal = 0,
    DeadlockSearch = 1,
    BaseSearch = 2,
    KDistSearch = 3,
    MiniSearch = 4,
    ForwardWithBases = 5,
    GirlSearch = 6,
    RevSearch = 7,
    Dragonfly = 8,
    WobblersSearch = 9,
    HfSearch = 10,
    RoomsDeadlockSearch = 11,
    NaiveSearch = 12,
    XySearch = 13,
    MiniCorral = 14,
    IgnoreCorral = 15,
    BiconSearch = 16,
    MaxDistSearch = 17,
    MaxDistSearch2 = 18,
    SnailSearch = 19,
    NetlockSearch = 20,
}

impl SearchMode {
    pub const ALL: [SearchMode; 21] = [
        SearchMode::Normal,
        SearchMode::DeadlockSearch,
        SearchMode::BaseSearch,
        SearchMode::KDistSearch,
        SearchMode::MiniSearch,
        SearchMode::ForwardWithBases,
        SearchMode::GirlSearch,
        SearchMode::RevSearch,
        SearchMode::Dragonfly,
        SearchMode::WobblersSearch,
        SearchMode::HfSearch,
        SearchMode::RoomsDeadlockSearch,
        SearchMode::NaiveSearch,
        SearchMode::XySearch,
        SearchMode::MiniCorral,
        SearchMode::IgnoreCorral,
        SearchMode::BiconSearch,
        SearchMode::MaxDistSearch,
        SearchMode::MaxDistSearch2,
        SearchMode::SnailSearch,
        SearchMode::NetlockSearch,
    ];

    pub fn from_i32(value: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| *mode as i32 == value)
    }

    pub fn pull_mode(self, forward: bool) -> bool {
        !forward
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_match_cpp() {
        assert_eq!(OCCUPIED, 3);
        assert_eq!(PACKED_BOX, 6);
        assert_eq!(SOKOBAN_ON_TARGET, 12);
    }

    #[test]
    fn search_mode_round_trip() {
        for mode in SearchMode::ALL {
            assert_eq!(SearchMode::from_i32(mode as i32), Some(mode));
        }
        assert_eq!(SearchMode::from_i32(21), None);
    }
}
