//! Graceful shutdown coordination for connected clients.
//!
//! Tracks active WebSocket connections and provides a mechanism to:
//! 1. Signal all connections to close
//! 2. Wait until all connections have actually closed

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Notify};

/// Coordinates graceful shutdown of client connections.
#[derive(Clone)]
pub struct ShutdownCoordinator {
    inner: Arc<Inner>,
}

struct Inner {
    /// Signals shutdown to all listeners
    shutdown_tx: watch::Sender<bool>,
    /// Active connection count
    active: AtomicUsize,
    /// Notified when all connections close
    all_closed: Notify,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            inner: Arc::new(Inner {
                shutdown_tx,
                active: AtomicUsize::new(0),
                all_closed: Notify::new(),
            }),
        }
    }

    /// Register a new connection. Returns a guard that must be held for the
    /// connection's lifetime, and a receiver for the shutdown signal.
    pub fn register(&self) -> (ConnectionGuard, watch::Receiver<bool>) {
        self.inner.active.fetch_add(1, Ordering::SeqCst);
        let guard = ConnectionGuard {
            inner: self.inner.clone(),
        };
        let shutdown_rx = self.inner.shutdown_tx.subscribe();
        (guard, shutdown_rx)
    }

    /// Signal all connections to shut down.
    pub fn shutdown(&self) {
        let _ = self.inner.shutdown_tx.send(true);
    }

    /// Wait until all connections have closed.
    /// Returns immediately if there are no active connections.
    pub async fn wait_for_all_closed(&self) {
        loop {
            let count = self.inner.active.load(Ordering::SeqCst);
            if count == 0 {
                return;
            }
            tracing::debug!(count, "waiting for connections to close");
            self.inner.all_closed.notified().await;
        }
    }

    /// Returns the current number of active connections.
    pub fn active_count(&self) -> usize {
        self.inner.active.load(Ordering::SeqCst)
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that decrements connection count when dropped.
pub struct ConnectionGuard {
    inner: Arc<Inner>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let prev = self.inner.active.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            // We were the last connection, notify waiters
            self.inner.all_closed.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_no_connections_returns_immediately() {
        let coord = ShutdownCoordinator::new();
        coord.shutdown();
        // Should not block
        coord.wait_for_all_closed().await;
    }

    #[tokio::test]
    async fn test_wait_for_connection_to_close() {
        let coord = ShutdownCoordinator::new();
        let (guard, mut shutdown_rx) = coord.register();

        assert_eq!(coord.active_count(), 1);

        // Signal shutdown
        coord.shutdown();
        assert!(*shutdown_rx.borrow_and_update());

        // Spawn wait task
        let coord_clone = coord.clone();
        let wait_task = tokio::spawn(async move {
            coord_clone.wait_for_all_closed().await;
        });

        // Give wait task a moment to start waiting
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!wait_task.is_finished());

        // Drop the guard (connection closed)
        drop(guard);

        // Wait should complete
        tokio::time::timeout(Duration::from_millis(100), wait_task)
            .await
            .expect("should complete")
            .expect("should not panic");

        assert_eq!(coord.active_count(), 0);
    }

    #[tokio::test]
    async fn test_multiple_connections() {
        let coord = ShutdownCoordinator::new();
        let (guard1, _) = coord.register();
        let (guard2, _) = coord.register();
        let (guard3, _) = coord.register();

        assert_eq!(coord.active_count(), 3);

        coord.shutdown();

        let coord_clone = coord.clone();
        let wait_task = tokio::spawn(async move {
            coord_clone.wait_for_all_closed().await;
        });

        // Drop connections one by one
        drop(guard1);
        assert_eq!(coord.active_count(), 2);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(!wait_task.is_finished());

        drop(guard2);
        assert_eq!(coord.active_count(), 1);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(!wait_task.is_finished());

        drop(guard3);
        assert_eq!(coord.active_count(), 0);

        // Now wait should complete
        tokio::time::timeout(Duration::from_millis(100), wait_task)
            .await
            .expect("should complete")
            .expect("should not panic");
    }

    #[tokio::test]
    async fn test_shutdown_signal_received() {
        let coord = ShutdownCoordinator::new();
        let (_guard, mut shutdown_rx) = coord.register();

        // Initially false
        assert!(!*shutdown_rx.borrow());

        // Signal shutdown
        coord.shutdown();

        // Should receive true
        shutdown_rx.changed().await.unwrap();
        assert!(*shutdown_rx.borrow());
    }
}
