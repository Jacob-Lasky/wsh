use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// Tracks the timestamp of the last terminal activity (PTY output or input).
///
/// Clients can wait for "quiescence" — a period of inactivity exceeding a
/// specified timeout — to detect when a command has finished producing output.
#[derive(Clone)]
pub struct ActivityTracker {
    tx: Arc<watch::Sender<Instant>>,
}

impl ActivityTracker {
    /// Create a new tracker seeded with the current instant.
    pub fn new() -> Self {
        let (tx, _) = watch::channel(Instant::now());
        Self { tx: Arc::new(tx) }
    }

    /// Record activity. Safe to call from blocking threads.
    pub fn touch(&self) {
        self.tx.send_replace(Instant::now());
    }

    /// Subscribe to activity changes. Returns a watch receiver that gets
    /// notified each time `touch()` is called.
    pub fn subscribe(&self) -> watch::Receiver<Instant> {
        self.tx.subscribe()
    }

    /// Wait until `timeout` has elapsed since the last activity.
    ///
    /// If the terminal has already been quiet for `timeout` when called,
    /// returns immediately.
    pub async fn wait_for_quiescence(&self, timeout: Duration) {
        let mut rx = self.tx.subscribe();
        loop {
            let last = *rx.borrow_and_update();
            let elapsed = last.elapsed();
            if elapsed >= timeout {
                return;
            }
            let remaining = timeout - elapsed;
            tokio::select! {
                _ = tokio::time::sleep(remaining) => {
                    // Double-check: a touch may have arrived in the tiny window
                    // between sleep completing and us running.
                    let last = *rx.borrow_and_update();
                    if last.elapsed() >= timeout {
                        return;
                    }
                    // Not yet quiescent — loop again with fresh remaining.
                }
                res = rx.changed() => {
                    if res.is_err() {
                        // Sender dropped — treat as quiescent.
                        return;
                    }
                    // Activity detected — loop to recalculate remaining.
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn touch_updates_state() {
        let tracker = ActivityTracker::new();
        let before = Instant::now();
        tokio::time::sleep(Duration::from_millis(10)).await;
        tracker.touch();
        // The last activity should be after `before`.
        let mut rx = tracker.tx.subscribe();
        let last = *rx.borrow_and_update();
        assert!(last > before);
    }

    #[tokio::test]
    async fn quiescence_fires_after_timeout() {
        let tracker = ActivityTracker::new();
        tracker.touch();
        let start = Instant::now();
        tracker.wait_for_quiescence(Duration::from_millis(50)).await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(50));
    }

    #[tokio::test]
    async fn activity_resets_timer() {
        let tracker = ActivityTracker::new();
        tracker.touch();

        let t = tracker.clone();
        // Spawn a task that touches after 30ms, resetting the timer.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            t.touch();
        });

        let start = Instant::now();
        tracker.wait_for_quiescence(Duration::from_millis(50)).await;
        let elapsed = start.elapsed();
        // Should take at least 80ms total (30ms + 50ms timeout from second touch).
        assert!(elapsed >= Duration::from_millis(75));
    }

    #[tokio::test]
    async fn already_quiescent_returns_immediately() {
        let tracker = ActivityTracker::new();
        // Don't touch — the seed instant was set at construction time.
        // Wait long enough that the seed is stale.
        tokio::time::sleep(Duration::from_millis(60)).await;

        let start = Instant::now();
        tracker.wait_for_quiescence(Duration::from_millis(50)).await;
        let elapsed = start.elapsed();
        // Should return almost immediately.
        assert!(elapsed < Duration::from_millis(10));
    }

    #[tokio::test]
    async fn multiple_concurrent_waiters() {
        let tracker = ActivityTracker::new();
        tracker.touch();

        let t1 = tracker.clone();
        let t2 = tracker.clone();

        let (r1, r2) = tokio::join!(
            async move {
                let start = Instant::now();
                t1.wait_for_quiescence(Duration::from_millis(50)).await;
                start.elapsed()
            },
            async move {
                let start = Instant::now();
                t2.wait_for_quiescence(Duration::from_millis(50)).await;
                start.elapsed()
            },
        );

        assert!(r1 >= Duration::from_millis(50));
        assert!(r2 >= Duration::from_millis(50));
    }
}
