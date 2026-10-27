// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

//! Score payload carried by search nodes (`score.h:score_element`).
//!
//! The struct is defined before the FESS engine because the distance-matching
//! helpers already write into it. Field types mirror the C++ `short`/`int`.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScoreElement {
    pub connectivity: i16,
    pub rooms_score: i16,
    pub packed_boxes: i16,
    pub boxes_on_targets: i16,
    pub boxes_in_level: i16,
    pub dist_to_targets: i32,
    pub dist_to_imagined: i32,
    pub hotspots: i16,
    pub out_of_plan: i16,
    pub overlap: i16,
    pub imagine: i16,
    pub value: f32,
    pub biconnect: i16,
}
