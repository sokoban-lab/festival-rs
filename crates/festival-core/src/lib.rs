// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Core algorithms of the Festival Sokoban solver.
//!
//! The crate is `#![forbid(unsafe_code)]`: all state is either owned by a
//! `Level`/`LevelContext`, or by a per-thread search context (added in later
//! phases). There is no global mutable state.

#![forbid(unsafe_code)]

pub mod analysis;
pub mod bfs;
pub mod board;
pub mod constants;
pub mod deadlock;
pub mod distance;
pub mod error;
pub mod graph;
pub mod io;
pub mod level;
pub mod moves;
pub mod search;
pub mod solver;

pub use board::Board;
pub use constants::*;
pub use error::{Result, SolveError};
pub use level::{Level, RawLevel};
pub use solver::{LevelContext, SearchClock};
