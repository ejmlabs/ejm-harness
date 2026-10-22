use std::time::{Duration, Instant};

/// Wait before the next attempt, doubling each time.
pub fn retry_backoff(attempt: u32) -> Duration {
    Duration::from_millis(100 << attempt.min(6))
}

/// True while the retry window is still open.
pub fn window_open(started: Instant, now: Instant, budget: Duration) -> bool {
    now.duration_since(started) < budget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_is_open_at_start() {
        let started = Instant::now();
        assert!(window_open(started, started, Duration::from_millis(1)));
    }

    #[test]
    fn window_closes_at_budget() {
        let started = Instant::now();
        let budget = Duration::from_millis(5);
        assert!(!window_open(started, started + budget, budget));
    }

    #[test]
    fn window_is_open_before_budget() {
        let started = Instant::now();
        let now = started + Duration::from_millis(4);
        assert!(window_open(started, now, Duration::from_millis(5)));
    }
}
