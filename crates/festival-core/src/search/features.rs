//! FESS feature cells (`request.h` / `request.cpp`).
//!
//! A request is the subset of a score that defines one feature-space cell.
//! Requests are consumed round-robin, matching the C++ ring behavior.
//! Small request sets (`< INLINE_THRESHOLD`, covering >90% of levels) scan a
//! few L1 cache lines without allocating a hash table; larger sets build and
//! maintain a `FastMap<u64, u16>` for `O(1)` lookup up to `MAX_REQUESTS`.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::constants::MAX_REQUESTS;
use crate::error::{Result, SolveError};
use crate::search::score::ScoreElement;

const INLINE_THRESHOLD: usize = 32;

#[derive(Default)]
struct U64Hasher(u64);

impl Hasher for U64Hasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.wrapping_mul(0x100000001b3) ^ (b as u64);
        }
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        let mut x = i;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        self.0 = x ^ (x >> 31);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<U64Hasher>>;

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

#[inline]
fn request_key(r: &Request) -> u64 {
    ((r.out_of_plan as u16 as u64) << 48)
        | ((r.packed_boxes as u16 as u64) << 32)
        | ((r.connectivity as u16 as u64) << 16)
        | (r.imagine as u16 as u64)
}

#[inline]
fn pack_request_key(r: &PackRequest) -> u64 {
    ((r.boxes_in_level as u16 as u64) << 48)
        | ((r.boxes_on_targets as u16 as u64) << 32)
        | ((r.connectivity as u16 as u64) << 16)
        | (r.room_connectivity as u16 as u64)
}

#[derive(Debug, Default)]
pub struct RequestData {
    requests: Vec<Request>,
    request_index: FastMap<u64, u16>,
    requests_pos: usize,
    pack_requests: Vec<PackRequest>,
    pack_request_index: FastMap<u64, u16>,
    pack_requests_pos: usize,
}

impl RequestData {
    pub fn clear(&mut self) {
        self.requests.clear();
        self.request_index.clear();
        self.requests_pos = 0;
        self.pack_requests.clear();
        self.pack_request_index.clear();
        self.pack_requests_pos = 0;
    }

    pub fn requests_len(&self) -> usize {
        self.requests.len()
    }

    #[inline]
    fn find_req_struct(&self, target: &Request) -> Option<usize> {
        if self.requests.len() < INLINE_THRESHOLD {
            self.requests.iter().position(|r| r == target)
        } else {
            self.request_index
                .get(&request_key(target))
                .map(|&idx| idx as usize)
        }
    }

    #[inline]
    fn find_pack_req_struct(&self, target: &PackRequest) -> Option<usize> {
        if self.pack_requests.len() < INLINE_THRESHOLD {
            self.pack_requests.iter().position(|r| r == target)
        } else {
            self.pack_request_index
                .get(&pack_request_key(target))
                .map(|&idx| idx as usize)
        }
    }

    #[inline]
    fn find_request(&self, s: &ScoreElement) -> Option<usize> {
        self.find_req_struct(&Request {
            out_of_plan: s.out_of_plan,
            packed_boxes: s.packed_boxes,
            connectivity: s.connectivity,
            imagine: s.imagine,
        })
    }

    #[inline]
    fn find_pack_request(&self, s: &ScoreElement) -> Option<usize> {
        self.find_pack_req_struct(&PackRequest {
            boxes_in_level: s.boxes_in_level,
            boxes_on_targets: s.boxes_on_targets,
            connectivity: s.connectivity,
            room_connectivity: s.rooms_score,
        })
    }

    pub fn add_request(&mut self, s: &ScoreElement) -> Result<i32> {
        let req = Request {
            out_of_plan: s.out_of_plan,
            packed_boxes: s.packed_boxes,
            connectivity: s.connectivity,
            imagine: s.imagine,
        };
        if let Some(existing) = self.find_req_struct(&req) {
            return Ok(existing as i32);
        }
        if self.requests.len() >= MAX_REQUESTS {
            return Err(SolveError::Internal("max requests".into()));
        }
        let idx = self.requests.len();
        self.requests.push(req);
        if self.requests.len() == INLINE_THRESHOLD {
            self.request_index.reserve(INLINE_THRESHOLD * 2);
            for (i, r) in self.requests.iter().enumerate() {
                self.request_index.insert(request_key(r), i as u16);
            }
        } else if self.requests.len() > INLINE_THRESHOLD {
            self.request_index.insert(request_key(&req), idx as u16);
        }
        self.requests_pos = idx;
        Ok(idx as i32)
    }

    pub fn add_pack_request(&mut self, s: &ScoreElement) -> Result<i32> {
        let req = PackRequest {
            boxes_in_level: s.boxes_in_level,
            boxes_on_targets: s.boxes_on_targets,
            connectivity: s.connectivity,
            room_connectivity: s.rooms_score,
        };
        if let Some(existing) = self.find_pack_req_struct(&req) {
            return Ok(existing as i32);
        }
        if self.pack_requests.len() >= MAX_REQUESTS {
            return Err(SolveError::Internal("max pack requests".into()));
        }
        let idx = self.pack_requests.len();
        self.pack_requests.push(req);
        if self.pack_requests.len() == INLINE_THRESHOLD {
            self.pack_request_index.reserve(INLINE_THRESHOLD * 2);
            for (i, r) in self.pack_requests.iter().enumerate() {
                self.pack_request_index
                    .insert(pack_request_key(r), i as u16);
            }
        } else if self.pack_requests.len() > INLINE_THRESHOLD {
            self.pack_request_index
                .insert(pack_request_key(&req), idx as u16);
        }
        self.pack_requests_pos = idx;
        Ok(idx as i32)
    }

    #[inline]
    pub fn next_request_label(&mut self) -> Option<i32> {
        if self.requests.is_empty() {
            return None;
        }
        let index = self.requests_pos;
        self.requests_pos += 1;
        if self.requests_pos == self.requests.len() {
            self.requests_pos = 0;
        }
        Some(index as i32)
    }

    #[inline]
    pub fn next_pack_request_label(&mut self) -> Option<i32> {
        if self.pack_requests.is_empty() {
            return None;
        }
        let index = self.pack_requests_pos;
        self.pack_requests_pos += 1;
        if self.pack_requests_pos == self.pack_requests.len() {
            self.pack_requests_pos = 0;
        }
        Some(index as i32)
    }

    pub fn next_request(&mut self) -> Option<Request> {
        self.next_request_label()
            .map(|index| self.requests[index as usize])
    }

    pub fn next_pack_request(&mut self) -> Option<PackRequest> {
        self.next_pack_request_label()
            .map(|index| self.pack_requests[index as usize])
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
    #[inline]
    pub fn label_of_score(&self, s: &ScoreElement, pull_mode: bool) -> i32 {
        let found = if pull_mode {
            self.find_pack_request(s)
        } else {
            self.find_request(s)
        };
        found.map(|index| index as i32).unwrap_or(-1)
    }

    #[inline]
    pub fn label_of_request(&self, request: &Request) -> i32 {
        self.find_req_struct(request)
            .map(|idx| idx as i32)
            .unwrap_or(-1)
    }

    #[inline]
    pub fn label_of_pack_request(&self, request: &PackRequest) -> i32 {
        self.find_pack_req_struct(request)
            .map(|idx| idx as i32)
            .unwrap_or(-1)
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
        assert_eq!(data.add_request(&score(1, 2)).unwrap(), 0);
        assert_eq!(data.add_request(&score(1, 2)).unwrap(), 0);
        assert_eq!(data.add_request(&score(1, 3)).unwrap(), 1);
        assert_eq!(data.requests_len(), 2);
    }

    #[test]
    fn requests_cycle() {
        let mut data = RequestData::default();
        data.add_request(&score(0, 0)).unwrap();
        data.add_request(&score(1, 0)).unwrap();
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

    #[test]
    fn hybrid_inline_and_hash_index_matches_linear_reference() {
        let mut data = RequestData::default();
        let mut ref_vec: Vec<Request> = Vec::new();
        let mut ref_pos = 0usize;
        let mut rng = 0x1234_5678_9abc_def0u64;

        for step in 0..1000 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let s = ScoreElement {
                out_of_plan: ((rng >> 48) as i16) % 7 - 3,
                packed_boxes: ((rng >> 32) as i16) % 9,
                connectivity: ((rng >> 16) as i16) % 5,
                imagine: (rng as i16) % 6 - 2,
                ..ScoreElement::default()
            };
            let req = Request {
                out_of_plan: s.out_of_plan,
                packed_boxes: s.packed_boxes,
                connectivity: s.connectivity,
                imagine: s.imagine,
            };
            let ref_label = if let Some(idx) = ref_vec.iter().position(|r| *r == req) {
                idx as i32
            } else {
                ref_vec.push(req);
                ref_pos = ref_vec.len() - 1;
                ref_pos as i32
            };
            let got_label = data.add_request(&s).unwrap();
            assert_eq!(got_label, ref_label);
            assert_eq!(data.label_of_score(&s, false), ref_label);
            assert_eq!(data.label_of_request(&req), ref_label);

            if step % 3 == 0 {
                let expected = ref_pos as i32;
                ref_pos = (ref_pos + 1) % ref_vec.len();
                assert_eq!(data.next_request_label(), Some(expected));
            }
        }
        assert!(data.requests_len() > INLINE_THRESHOLD);
    }
}
