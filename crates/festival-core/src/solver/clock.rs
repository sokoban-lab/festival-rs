use std::time::{Duration, Instant};

/// Time/expansion budget for a solve attempt.
///
/// The C++ solver uses whole seconds and checks once per main-loop iteration;
/// `expired` keeps the same `elapsed > limit` semantics.
#[derive(Clone, Debug)]
pub struct SearchClock {
    started: Instant,
    time_limit: Option<Duration>,
    max_expansions: Option<u64>,
}

impl SearchClock {
    pub fn new(time_limit: Option<Duration>, max_expansions: Option<u64>) -> Self {
        Self {
            started: Instant::now(),
            time_limit,
            max_expansions,
        }
    }

    pub fn started(&self) -> Instant {
        self.started
    }

    pub fn time_limit(&self) -> Option<Duration> {
        self.time_limit
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn expired(&self) -> bool {
        match self.time_limit {
            Some(limit) => self.elapsed() > limit,
            None => false,
        }
    }

    pub fn should_stop(&self, expansions: u64) -> bool {
        if let Some(max) = self.max_expansions {
            if expansions >= max {
                return true;
            }
        }
        self.expired()
    }

    /// Hotspots are aborted after a quarter of the level time, matching
    /// `hotspots_take_too_much_time`.
    pub fn hotspot_abort_at(&self) -> Option<Instant> {
        self.time_limit.map(|limit| self.started + limit / 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_budget_stops() {
        let clock = SearchClock::new(None, Some(10));
        assert!(!clock.should_stop(9));
        assert!(clock.should_stop(10));
    }

    #[test]
    fn no_limits_never_expires() {
        let clock = SearchClock::new(None, None);
        assert!(!clock.expired());
        assert!(!clock.should_stop(u64::MAX));
    }
}
