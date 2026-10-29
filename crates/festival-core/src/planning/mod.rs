//! Reverse-search planning data: parking order, OOP zones, imagined boards.
//!
//! The C++ solver keeps these in `helper` (`park_order.cpp`, `oop.cpp`,
//! `imagine.cpp`, `overlap.cpp`, `heuristics.cpp`). They are per-thread state
//! and live in [`crate::search::helper::Helper`].

pub mod heuristics;
pub mod imagine;
pub mod oop;
pub mod opener;
pub mod overlap;
pub mod park_order;

use crate::board::Board;

/// One step of a parking order (`park_order_data`).
///
/// `from == -2` marks a box that was already in its initial place, `-1` marks
/// a box whose origin is irrelevant (permanent placement after the plan was
/// reduced to a permutation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParkOrder {
    pub from: i32,
    pub to: i32,
    pub until: i32,
}

impl Default for ParkOrder {
    fn default() -> Self {
        Self {
            from: -1,
            to: -1,
            until: -1,
        }
    }
}

/// Per-step out-of-plan zones (`oop_data`).
#[derive(Debug, Default)]
pub struct OopData {
    /// `zones[i]`: cells that must be cleared before step `i`.
    pub zones: Vec<Board>,
    /// `distances[i]`: push distance to `zones[i]` (`1000000` = unreachable).
    pub distances: Vec<Vec<u32>>,
}

/// Imagined boards used by the forward planning score (`imagine_data`).
#[derive(Debug, Default)]
pub struct ImagineData {
    pub imagined_boards: Vec<Board>,
    pub fixed_at_step: Vec<Board>,
    pub imagine_packed_num: Vec<usize>,
}
