//! FESS feature cells (`request.h` / `request.cpp`).
//!
//! A request is the subset of a score that defines one feature-space cell.
//! Requests are consumed round-robin, matching the C++ ring behavior.

use crate::constants::MAX_REQUESTS;
use crate::error::{Result, SolveError};
use crate::search::score::ScoreElement;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub out_of_plan: i16,
    pub packed_boxes: i16,
    pub connectivity: i16,
    pub imagine: i16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackRequest {
    pub boxes_in_level: i16,
    pub boxes_on_targets: i16,
    pub connectivity: i16,
    pub room_connectivity: i16,
}

#[derive(Debug, Default)]
pub struct RequestData {
    requests: Vec<Request>,
    requests_pos: usize,
    pack_requests: Vec<PackRequest>,
    pack_requests_pos: usize,
}

impl RequestData {
    pub fn clear(&mut self) {
        self.requests.clear();
        self.requests_pos = 0;
        self.pack_requests.clear();
        self.pack_requests_pos = 0;
    }

    pub fn requests_len(&self) -> usize {
        self.requests.len()
    }

    fn request_matches(req: &Request, s: &ScoreElement) -> bool {
        req.packed_boxes == s.packed_boxes
            && req.connectivity == s.connectivity
            && req.out_of_plan == s.out_of_plan
            && req.imagine == s.imagine
    }

    fn pack_request_matches(req: &PackRequest, s: &ScoreElement) -> bool {
        req.boxes_in_level == s.boxes_in_level
            && req.boxes_on_targets == s.boxes_on_targets
            && req.connectivity == s.connectivity
            && req.room_connectivity == s.rooms_score
    }

    fn find_request(&self, s: &ScoreElement) -> Option<usize> {
        self.requests
            .iter()
            .position(|req| Self::request_matches(req, s))
    }

    fn find_pack_request(&self, s: &ScoreElement) -> Option<usize> {
        self.pack_requests
            .iter()
            .position(|req| Self::pack_request_matches(req, s))
    }

    pub fn add_request(&mut self, s: &ScoreElement) -> Result<()> {
        if self.find_request(s).is_some() {
            return Ok(());
        }
        if self.requests.len() >= MAX_REQUESTS {
            return Err(SolveError::Internal("max requests".into()));
        }
        self.requests.push(Request {
            out_of_plan: s.out_of_plan,
            packed_boxes: s.packed_boxes,
            connectivity: s.connectivity,
            imagine: s.imagine,
        });
        self.requests_pos = self.requests.len() - 1;
        Ok(())
    }

    pub fn add_pack_request(&mut self, s: &ScoreElement) -> Result<()> {
        if self.find_pack_request(s).is_some() {
            return Ok(());
        }
        if self.pack_requests.len() >= MAX_REQUESTS {
            return Err(SolveError::Internal("max pack requests".into()));
        }
        self.pack_requests.push(PackRequest {
            boxes_in_level: s.boxes_in_level,
            boxes_on_targets: s.boxes_on_targets,
            connectivity: s.connectivity,
            room_connectivity: s.rooms_score,
        });
        self.pack_requests_pos = self.pack_requests.len() - 1;
        Ok(())
    }

    pub fn next_request(&mut self) -> Option<Request> {
        if self.requests.is_empty() {
            return None;
        }
        let request = self.requests[self.requests_pos];
        self.requests_pos += 1;
        if self.requests_pos == self.requests.len() {
            self.requests_pos = 0;
        }
        Some(request)
    }

    pub fn next_pack_request(&mut self) -> Option<PackRequest> {
        if self.pack_requests.is_empty() {
            return None;
        }
        let request = self.pack_requests[self.pack_requests_pos];
        self.pack_requests_pos += 1;
        if self.pack_requests_pos == self.pack_requests.len() {
            self.pack_requests_pos = 0;
        }
        Some(request)
    }

    pub fn request_of_score(&self, s: &ScoreElement) -> Option<Request> {
        self.find_request(s).map(|index| self.requests[index])
    }

    pub fn pack_request_of_score(&self, s: &ScoreElement) -> Option<PackRequest> {
        self.find_pack_request(s)
            .map(|index| self.pack_requests[index])
    }

    pub fn set_next_request_to_score(&mut self, s: &ScoreElement) -> bool {
        match self.find_request(s) {
            Some(index) => {
                self.requests_pos = index;
                true
            }
            None => false,
        }
    }

    pub fn set_next_pack_request_to_score(&mut self, s: &ScoreElement) -> bool {
        match self.find_pack_request(s) {
            Some(index) => {
                self.pack_requests_pos = index;
                true
            }
            None => false,
        }
    }

    /// Label of a score inside the active feature space, or `-1`.
    pub fn label_of_score(&self, s: &ScoreElement, pull_mode: bool) -> i32 {
        let found = if pull_mode {
            self.find_pack_request(s)
        } else {
            self.find_request(s)
        };
        found.map(|index| index as i32).unwrap_or(-1)
    }

    pub fn label_of_request(&self, request: &Request) -> i32 {
        let s = ScoreElement {
            packed_boxes: request.packed_boxes,
            connectivity: request.connectivity,
            out_of_plan: request.out_of_plan,
            imagine: request.imagine,
            ..ScoreElement::default()
        };
        self.label_of_score(&s, false)
    }

    pub fn label_of_pack_request(&self, request: &PackRequest) -> i32 {
        let s = ScoreElement {
            boxes_in_level: request.boxes_in_level,
            boxes_on_targets: request.boxes_on_targets,
            connectivity: request.connectivity,
            rooms_score: request.room_connectivity,
            ..ScoreElement::default()
        };
        self.label_of_score(&s, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(packed: i16, connectivity: i16) -> ScoreElement {
        ScoreElement {
            packed_boxes: packed,
            connectivity,
            ..ScoreElement::default()
        }
    }

    #[test]
    fn requests_are_deduplicated() {
        let mut data = RequestData::default();
        data.add_request(&score(1, 2)).unwrap();
        data.add_request(&score(1, 2)).unwrap();
        data.add_request(&score(1, 3)).unwrap();
        assert_eq!(data.requests_len(), 2);
    }

    #[test]
    fn requests_cycle() {
        let mut data = RequestData::default();
        data.add_request(&score(0, 0)).unwrap();
        data.add_request(&score(1, 0)).unwrap();
        // Adding the second request leaves the cursor on it.
        assert_eq!(data.next_request().unwrap().packed_boxes, 1);
        assert_eq!(data.next_request().unwrap().packed_boxes, 0);
        assert_eq!(data.next_request().unwrap().packed_boxes, 1);
    }

    #[test]
    fn labels_follow_requests() {
        let mut data = RequestData::default();
        data.add_request(&score(0, 1)).unwrap();
        data.add_request(&score(2, 1)).unwrap();
        assert_eq!(data.label_of_score(&score(2, 1), false), 1);
        assert_eq!(data.label_of_score(&score(9, 9), false), -1);
    }
}
