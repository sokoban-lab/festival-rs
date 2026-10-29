use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod advisors;
pub mod dragonfly;
pub mod features;
pub mod fess;
pub mod girl;
pub mod helper;
pub mod hf;
pub mod max_dist;
pub mod naive;
pub mod packing;
pub mod perimeter;
pub mod queue;
pub mod score;
pub mod snail;
pub mod tree;

pub use fess::{FessConfig, FessRun, run_fess};
pub use score::ScoreElement;

/// C++ `any_core_solved`: every search loop polls the shared flag.
#[inline]
pub(crate) fn cancelled(flag: &Option<Arc<AtomicBool>>) -> bool {
    flag.as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
}
