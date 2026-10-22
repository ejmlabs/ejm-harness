use std::time::{Duration, Instant};

/// Wait before the next attempt, doubling each time.
pub fn retry_backoff(attempt: u32) -> Duration {
    Duration::from_millis(100 << attempt.min(6))
}

/// True while the retry window is still open.
pub fn window_open(started: Instant, budget: Duration) -> bool {
    let now = Instant::now();
    now.duration_since(started) < budget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_is_open_at_start() {
        let started = Instant::now();
        assert!(window_open(started, Duration::from_millis(1)));
    }
}
