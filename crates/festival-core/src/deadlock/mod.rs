// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

pub mod cache;
pub mod corral;
pub mod corral_search;
pub mod cycle;
pub mod diags;
pub mod direction;
pub mod fixed_boxes;
pub mod k_dist;
pub mod mini;
pub mod mpdb;
pub mod oracle;
pub mod patterns;
pub mod positions;
pub mod rooms;
pub mod state;
pub mod static_checks;
pub mod stuck;
pub mod wobblers;
pub mod xy;
pub mod zone;

pub const NOT_DEADLOCK: i8 = 0;
pub const IS_DEADLOCK: i8 = 1;
pub const SEARCH_EXCEEDED: i8 = 2;
