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

    /// Indices of all boxes, in index order (`get_indices_of_boxes`).
    pub fn indices_of_boxes(&self, board: &crate::board::Board) -> Vec<usize> {
        self.indices_where(|y, x| board.cell(y, x) & crate::constants::BOX != 0)
    }

    /// Indices of all targets, in index order (`get_indices_of_targets`).
    pub fn indices_of_targets(&self, board: &crate::board::Board) -> Vec<usize> {
        self.indices_where(|y, x| board.cell(y, x) & crate::constants::TARGET != 0)
    }

    /// Indices of the initial box positions (`get_indices_of_initial_bases`).
    pub fn indices_of_initial_bases(&self) -> Vec<usize> {
        self.indices_where(|y, x| self.initial.cell(y, x) & crate::constants::BOX != 0)
    }

    fn indices_where(&self, predicate: impl Fn(usize, usize) -> bool) -> Vec<usize> {
        let mut indices = Vec::new();
        for index in 0..self.index_num() {
            let (y, x) = self.coord(index);
            if predicate(y, x) {
                indices.push(index);
            }
        }
        indices
    }

    /// Port of `is_cyclic_level`: every box already sits on a target.
    ///
    /// Such a level starts solved, but the root is excluded from the solved
    /// check, so FESS searches for a cycle that reaches the solved state again.
    pub fn is_cyclic(&self) -> bool {
        for y in 0..self.initial.height() {
            for x in 0..self.initial.width() {
                let cell = self.initial.cell(y, x);
                if cell == crate::constants::BOX || cell == crate::constants::TARGET {
                    return false;
                }
            }
        }
        true
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

#[cfg(test)]
mod tests {
    use super::*;

    fn level(text: &str) -> Level {
        Level::preprocess(parse_levels(text).remove(0)).unwrap()
    }

    #[test]
    fn detects_cyclic_levels() {
        assert!(level("#######\n#@*   #\n#     #\n#######\n").is_cyclic());
        assert!(!level("#######\n#@ $ .#\n#######\n").is_cyclic());
    }
}
