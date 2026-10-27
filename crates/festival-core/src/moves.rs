// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveAttr {
    pub advisor: bool,
    /// Perimeter can make this negative, hence the wider type.
    pub weight: i16,
    pub reversible: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Move {
    pub from: u16,
    pub to: u16,
    pub sokoban_position: u8,
    pub kill: bool,
    pub base: bool,
    pub pull: bool,
    pub attr: MoveAttr,
}

pub fn attr_to_weight(attr: &MoveAttr) -> i16 {
    if attr.advisor { 0 } else { 1 }
}
