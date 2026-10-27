// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SolveError>;

/// Failure causes exposed in `times.txt`.
///
/// The strings in [`SolveError::reason`] must stay byte-compatible with the
/// C++ `global_fail_reason` values.
#[derive(Debug, Error)]
pub enum SolveError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid board")]
    InvalidBoard,
    #[error("Too many boxes")]
    TooManyBoxes,
    #[error("too many rooms")]
    TooManyRooms,
    #[error("Could not find packing order")]
    NoPackingOrder,
    #[error("Too many moves")]
    TooManyMoves,
    #[error("Max nodes reached")]
    ResourceExhausted,
    #[error("Time limit exceeded")]
    TimeLimit,
    #[error("Size is too big")]
    SizeTooBig,
    #[error("Level not found")]
    LevelNotFound,
    #[error("Invalid move: {0}")]
    InvalidMove(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl SolveError {
    /// C++ compatible reason string used by `times.txt`.
    pub fn reason(&self) -> &'static str {
        match self {
            SolveError::Parse(_) => "Invalid board",
            SolveError::InvalidBoard => "Invalid board",
            SolveError::TooManyBoxes => "Too many boxes",
            SolveError::TooManyRooms => "too many rooms",
            SolveError::NoPackingOrder => "Could not find packing order",
            SolveError::TooManyMoves => "Too many moves",
            SolveError::ResourceExhausted => "Max nodes reached",
            SolveError::TimeLimit => "Time limit exceeded",
            SolveError::SizeTooBig => "Size is too big",
            SolveError::LevelNotFound => "Unknown reason",
            SolveError::InvalidMove(_) => "Unknown reason",
            SolveError::Io(_) => "Unknown reason",
            SolveError::Internal(_) => "Unknown reason",
        }
    }
}
