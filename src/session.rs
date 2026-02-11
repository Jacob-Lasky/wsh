use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::activity::ActivityTracker;
use crate::input::{InputBroadcaster, InputMode};
use crate::overlay::OverlayStore;
use crate::panel::PanelStore;
use crate::parser::Parser;
use crate::pty::Pty;
use crate::shutdown::ShutdownCoordinator;
use crate::terminal::TerminalSize;

/// A single terminal session with all associated state.
///
/// Each `Session` owns the PTY, parser, I/O channels, and auxiliary stores
/// for one terminal session. In standalone mode there is exactly one session;
/// in server mode the `SessionRegistry` manages many.
#[derive(Clone)]
pub struct Session {
    /// Human-readable session name (displayed in UI, used in URLs).
    pub name: String,
    pub input_tx: mpsc::Sender<Bytes>,
    pub output_rx: broadcast::Sender<Bytes>,
    pub shutdown: ShutdownCoordinator,
    pub parser: Parser,
    pub overlays: OverlayStore,
    pub panels: PanelStore,
    pub pty: Arc<Pty>,
    pub terminal_size: TerminalSize,
    pub input_mode: InputMode,
    pub input_broadcaster: InputBroadcaster,
    pub activity: ActivityTracker,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::Broker;

    /// Helper: build a minimal Session suitable for unit tests.
    fn create_test_session(name: &str) -> (Session, mpsc::Receiver<Bytes>) {
        let (input_tx, input_rx) = mpsc::channel(64);
        let broker = Broker::new();
        let parser = Parser::spawn(&broker, 80, 24, 1000);
        let pty = crate::pty::Pty::spawn(24, 80, crate::pty::SpawnCommand::default())
            .expect("failed to spawn PTY for test");

        let session = Session {
            name: name.to_string(),
            input_tx,
            output_rx: broker.sender(),
            shutdown: ShutdownCoordinator::new(),
            parser,
            overlays: OverlayStore::new(),
            panels: PanelStore::new(),
            pty: Arc::new(pty),
            terminal_size: TerminalSize::new(24, 80),
            input_mode: InputMode::new(),
            input_broadcaster: InputBroadcaster::new(),
            activity: ActivityTracker::new(),
        };
        (session, input_rx)
    }

    #[tokio::test]
    async fn test_session_can_be_constructed_with_name() {
        let (session, _rx) = create_test_session("my-session");
        assert_eq!(session.name, "my-session");
    }

    #[tokio::test]
    async fn test_session_is_cloneable() {
        let (session, _rx) = create_test_session("clone-me");
        let cloned = session.clone();

        // Both copies share the same name.
        assert_eq!(cloned.name, "clone-me");

        // The underlying broadcast sender is shared (same channel).
        assert_eq!(
            session.output_rx.receiver_count(),
            cloned.output_rx.receiver_count(),
        );
    }
}
