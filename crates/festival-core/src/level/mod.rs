// Derived from Festival 3.1 (C++), Copyright 2018-2022 Yaron Shoham.
// Rust port: festival-rs contributors.

mod preprocess;

pub use crate::io::sok::{RawLevel, parse_levels};
pub use preprocess::Level;

use crate::constants::MAX_INNER;

impl Level {
    #[inline]
    pub fn index_num(&self) -> usize {
        self.coords.len()
    }

    #[inline]
    pub fn index_at(&self, y: usize, x: usize) -> Option<usize> {
        let value = self.index_of[y * crate::constants::MAX_SIZE + x];
        if value < 0 {
            None
        } else {
            Some(value as usize)
        }
    }

    #[inline]
    pub fn coord(&self, index: usize) -> (usize, usize) {
        let (y, x) = self.coords[index];
        (y as usize, x as usize)
    }

    #[inline]
    pub fn is_inner(&self, y: usize, x: usize) -> bool {
        self.inner[y * crate::constants::MAX_SIZE + x]
    }

    #[inline]
    pub fn inner_at(&self, y: usize, x: usize) -> bool {
        self.inner[y * crate::constants::MAX_SIZE + x]
    }

    pub fn inner_flat(&self) -> &[bool; MAX_INNER] {
        &self.inner
    }

    pub fn boxes_as_board(&self) -> crate::board::Board {
        let mut boxes = self.initial;
        for y in 0..boxes.height() {
            for x in 0..boxes.width() {
                let cell = boxes.cell(y, x);
                boxes.set(
                    y,
                    x,
                    if cell & crate::constants::BOX != 0 {
                        1
                    } else {
                        0
                    },
                );
            }
        }
        boxes
    }

    pub fn targets_as_board(&self) -> crate::board::Board {
        let mut targets = self.initial;
        for y in 0..targets.height() {
            for x in 0..targets.width() {
                let cell = targets.cell(y, x);
                targets.set(
                    y,
                    x,
                    if cell & crate::constants::TARGET != 0 {
                        1
                    } else {
                        0
                    },
                );
            }
        }
        targets
    }
}
