//! Label queues (`queue.cpp`).
//!
//! The comparator needs the current expansion scores, so heap operations take
//! the expansion slice explicitly. Ordering is: lower `best_weight`, then
//! `is_better_score`, then lower expansion index.

use crate::constants::SearchMode;
use crate::search::score::is_better_score;
use crate::search::tree::ExpansionData;

#[derive(Debug, Default)]
pub struct IndexedHeap {
    elements: Vec<u32>,
}

impl IndexedHeap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn clear(&mut self) {
        self.elements.clear();
    }

    #[inline]
    fn is_better(
        first: u32,
        second: u32,
        expansions: &[ExpansionData],
        pull_mode: bool,
        search_mode: SearchMode,
    ) -> bool {
        let e1 = &expansions[first as usize];
        let e2 = &expansions[second as usize];

        if e1.best_weight < e2.best_weight {
            return true;
        }
        if e1.best_weight > e2.best_weight {
            return false;
        }

        if is_better_score(&e1.best_score, &e2.best_score, pull_mode, search_mode) {
            return true;
        }
        if is_better_score(&e2.best_score, &e1.best_score, pull_mode, search_mode) {
            return false;
        }

        first < second
    }

    fn sift_up(
        &mut self,
        mut index: usize,
        expansions: &[ExpansionData],
        pull_mode: bool,
        search_mode: SearchMode,
    ) {
        let value = self.elements[index];
        while index > 0 {
            let parent = (index - 1) / 2;
            let parent_val = self.elements[parent];
            if !Self::is_better(value, parent_val, expansions, pull_mode, search_mode) {
                break;
            }
            self.elements[index] = parent_val;
            index = parent;
        }
        self.elements[index] = value;
    }

    fn sift_down(
        &mut self,
        mut index: usize,
        expansions: &[ExpansionData],
        pull_mode: bool,
        search_mode: SearchMode,
    ) {
        let size = self.elements.len();
        let value = self.elements[index];
        loop {
            let left = index * 2 + 1;
            if left >= size {
                break;
            }
            let right = left + 1;
            let mut best_child = left;
            let mut best_val = self.elements[left];

            if right < size {
                let right_val = self.elements[right];
                if Self::is_better(right_val, best_val, expansions, pull_mode, search_mode) {
                    best_child = right;
                    best_val = right_val;
                }
            }

            if !Self::is_better(best_val, value, expansions, pull_mode, search_mode) {
                break;
            }
            self.elements[index] = best_val;
            index = best_child;
        }
        self.elements[index] = value;
    }

    pub fn insert(
        &mut self,
        value: u32,
        expansions: &[ExpansionData],
        pull_mode: bool,
        search_mode: SearchMode,
    ) {
        let index = self.elements.len();
        self.elements.push(value);
        self.sift_up(index, expansions, pull_mode, search_mode);
    }

    pub fn pop_max(
        &mut self,
        expansions: &[ExpansionData],
        pull_mode: bool,
        search_mode: SearchMode,
    ) -> Option<u32> {
        let result = *self.elements.first()?;
        let last = self.elements.pop()?;
        if !self.elements.is_empty() {
            self.elements[0] = last;
            self.sift_down(0, expansions, pull_mode, search_mode);
        }
        Some(result)
    }

    /// Finds `value` starting from the root and leaf end (since
    /// `check_if_perimeter_reached` always updates `last_expansion()`, which
    /// was just inserted near the end of `elements`).
    #[inline]
    fn find_index(&self, value: u32) -> Option<usize> {
        if self.elements.first() == Some(&value) {
            return Some(0);
        }
        self.elements.iter().rposition(|&e| e == value)
    }

    /// Updates `value` in-place if present, or inserts it if absent.
    pub fn update_or_insert(
        &mut self,
        value: u32,
        expansions: &[ExpansionData],
        pull_mode: bool,
        search_mode: SearchMode,
    ) {
        if let Some(index) = self.find_index(value) {
            self.sift_down(index, expansions, pull_mode, search_mode);
            self.sift_up(index, expansions, pull_mode, search_mode);
        } else {
            self.insert(value, expansions, pull_mode, search_mode);
        }
    }

    /// Removes a value by replacing it with the last element and restoring the
    /// heap. Returns `false` when the value is not present.
    pub fn remove(
        &mut self,
        value: u32,
        expansions: &[ExpansionData],
        pull_mode: bool,
        search_mode: SearchMode,
    ) -> bool {
        let Some(index) = self.find_index(value) else {
            return false;
        };

        self.elements.swap_remove(index);
        if index < self.elements.len() {
            self.sift_down(index, expansions, pull_mode, search_mode);
            self.sift_up(index, expansions, pull_mode, search_mode);
        }
        true
    }

    #[cfg(test)]
    fn verify(&self, expansions: &[ExpansionData], pull_mode: bool, search_mode: SearchMode) {
        for index in 0..self.elements.len() {
            let left = index * 2 + 1;
            let right = index * 2 + 2;
            if left < self.elements.len() {
                assert!(!Self::is_better(
                    self.elements[left],
                    self.elements[index],
                    expansions,
                    pull_mode,
                    search_mode
                ));
            }
            if right < self.elements.len() {
                assert!(!Self::is_better(
                    self.elements[right],
                    self.elements[index],
                    expansions,
                    pull_mode,
                    search_mode
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::score::ScoreElement;
    use crate::search::tree::NONE_EXPANSION;

    fn expansion(index: u32, weight: i32, connectivity: i16) -> ExpansionData {
        ExpansionData {
            node: index,
            board_offset: 0,
            father: NONE_EXPANSION,
            moves_num: 0,
            move_hash_place: 0,
            best_move: -1,
            best_weight: weight,
            best_score: ScoreElement {
                connectivity,
                ..ScoreElement::default()
            },
            best_past: index,
            label: -1,
            has_corral: false,
            weight: 0,
            depth: 0,
            subtree_size: 0,
            next_son_to_check: 0,
            leaf_direction: -1,
            advisors: Default::default(),
        }
    }

    #[test]
    fn pops_lowest_weight_then_best_score() {
        let expansions = vec![
            expansion(0, 1, 5),
            expansion(1, 0, 9),
            expansion(2, 0, 3),
            expansion(3, 2, 0),
        ];
        let mut heap = IndexedHeap::new();
        for value in 0..4 {
            heap.insert(value, &expansions, false, SearchMode::Normal);
        }
        heap.verify(&expansions, false, SearchMode::Normal);
        assert_eq!(
            heap.pop_max(&expansions, false, SearchMode::Normal),
            Some(2)
        );
        assert_eq!(
            heap.pop_max(&expansions, false, SearchMode::Normal),
            Some(1)
        );
        assert_eq!(
            heap.pop_max(&expansions, false, SearchMode::Normal),
            Some(0)
        );
        assert_eq!(
            heap.pop_max(&expansions, false, SearchMode::Normal),
            Some(3)
        );
        assert_eq!(heap.pop_max(&expansions, false, SearchMode::Normal), None);
    }

    #[test]
    fn ties_break_on_index() {
        let expansions = vec![expansion(0, 0, 0), expansion(1, 0, 0)];
        let mut heap = IndexedHeap::new();
        heap.insert(1, &expansions, false, SearchMode::Normal);
        heap.insert(0, &expansions, false, SearchMode::Normal);
        assert_eq!(
            heap.pop_max(&expansions, false, SearchMode::Normal),
            Some(0)
        );
    }

    #[test]
    fn remove_and_update_restore_heap() {
        let mut expansions = vec![
            expansion(0, 0, 0),
            expansion(1, 1, 0),
            expansion(2, 2, 0),
            expansion(3, 3, 0),
        ];
        let mut heap = IndexedHeap::new();
        for value in 0..4 {
            heap.insert(value, &expansions, false, SearchMode::Normal);
        }
        assert!(heap.remove(2, &expansions, false, SearchMode::Normal));
        assert!(!heap.remove(2, &expansions, false, SearchMode::Normal));
        heap.verify(&expansions, false, SearchMode::Normal);
        assert_eq!(heap.len(), 3);

        expansions[3].best_weight = -1999;
        heap.update_or_insert(3, &expansions, false, SearchMode::Normal);
        heap.verify(&expansions, false, SearchMode::Normal);
        assert_eq!(
            heap.pop_max(&expansions, false, SearchMode::Normal),
            Some(3)
        );
    }
}
