//! Input mode state for controlling input routing.
//!
//! Provides thread-safe state for controlling whether keyboard input
//! goes to the PTY (passthrough mode) or only to API subscribers (capture mode).
//!
//! # Design decision: no auto-cleanup on client disconnect
//!
//! Input capture mode is intentionally NOT tied to any specific client
//! connection. Capture/release are plain idempotent operations with no
//! owner tracking. This is a deliberate design choice after evaluating
//! and reverting an owner-based approach (see git history: d0bf7dc added
//! auto-release on WS disconnect, 0990160 reverted it).
//!
//! **Why no auto-cleanup:**
//!
//! - **Multi-agent scenarios**: Multiple agents may share a session. If
//!   agent A captures input and agent B disconnects, auto-cleanup would
//!   release A's capture — cascading state loss.
//! - **Idempotency**: Owner tracking requires identity state, breaking
//!   the simple capture/release model. Clients would need to track and
//!   pass ownership tokens.
//! - **Protocol independence**: Capture mode works across HTTP, WS, MCP,
//!   and socket transports. Tying it to WS connection lifecycle would
//!   make it transport-dependent.
//!
//! **Recovery mechanisms:**
//!
//! - Ctrl+\ toggles capture mode from the local terminal (see
//!   `server.rs` StdinInput handler).
//! - Any client can call `release_input` at any time.
//! - The API `GET /sessions/:name/input/mode` lets agents check state.
//!
//! **Do not re-add auto-cleanup.** This has been evaluated and reverted.
//! If you believe the tradeoffs have changed, discuss before implementing.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;

/// The current input routing mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Input goes to both API subscribers and PTY
    #[default]
    Passthrough,
    /// Input goes only to API subscribers
    Capture,
}

/// Thread-safe input mode state.
///
/// This struct provides a way to control input routing from multiple threads.
/// It defaults to passthrough mode where input flows to both API subscribers
/// and the PTY.
#[derive(Clone)]
pub struct InputMode {
    inner: Arc<RwLock<Mode>>,
}

impl InputMode {
    /// Creates a new InputMode in the default Passthrough state.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Mode::default())),
        }
    }

    /// Gets the current mode.
    pub fn get(&self) -> Mode {
        *self.inner.read()
    }

    /// Sets the mode to Capture.
    pub fn capture(&self) {
        *self.inner.write() = Mode::Capture;
    }

    /// Sets the mode to Passthrough.
    pub fn release(&self) {
        *self.inner.write() = Mode::Passthrough;
    }

    /// Toggles the mode: Passthrough → Capture, Capture → Passthrough.
    ///
    /// Used by the local terminal user (Ctrl+\).
    /// Returns the new mode after toggling.
    pub fn toggle(&self) -> Mode {
        let mut guard = self.inner.write();
        let new_mode = match *guard {
            Mode::Passthrough => Mode::Capture,
            Mode::Capture => Mode::Passthrough,
        };
        *guard = new_mode;
        new_mode
    }

    /// Returns true if the current mode is Capture.
    pub fn is_capture(&self) -> bool {
        self.get() == Mode::Capture
    }
}

impl Default for InputMode {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_passthrough() {
        let input_mode = InputMode::new();
        assert_eq!(input_mode.get(), Mode::Passthrough);
    }

    #[test]
    fn test_capture_mode() {
        let input_mode = InputMode::new();
        input_mode.capture();
        assert_eq!(input_mode.get(), Mode::Capture);
    }

    #[test]
    fn test_release_mode() {
        let input_mode = InputMode::new();
        input_mode.capture();
        assert_eq!(input_mode.get(), Mode::Capture);
        input_mode.release();
        assert_eq!(input_mode.get(), Mode::Passthrough);
    }

    #[test]
    fn test_capture_is_idempotent() {
        let input_mode = InputMode::new();
        input_mode.capture();
        input_mode.capture();
        assert_eq!(input_mode.get(), Mode::Capture);
    }

    #[test]
    fn test_release_is_idempotent() {
        let input_mode = InputMode::new();
        input_mode.release();
        assert_eq!(input_mode.get(), Mode::Passthrough);
    }

    #[test]
    fn test_toggle() {
        let input_mode = InputMode::new();
        assert_eq!(input_mode.get(), Mode::Passthrough);

        let new_mode = input_mode.toggle();
        assert_eq!(new_mode, Mode::Capture);
        assert_eq!(input_mode.get(), Mode::Capture);

        let new_mode = input_mode.toggle();
        assert_eq!(new_mode, Mode::Passthrough);
        assert_eq!(input_mode.get(), Mode::Passthrough);
    }

    #[test]
    fn test_is_capture() {
        let input_mode = InputMode::new();
        assert!(!input_mode.is_capture());
        input_mode.capture();
        assert!(input_mode.is_capture());
        input_mode.release();
        assert!(!input_mode.is_capture());
    }

    #[test]
    fn test_clone_shares_state() {
        let input_mode1 = InputMode::new();
        let input_mode2 = input_mode1.clone();

        input_mode1.capture();
        assert_eq!(input_mode2.get(), Mode::Capture);

        input_mode2.release();
        assert_eq!(input_mode1.get(), Mode::Passthrough);
    }

    #[test]
    fn test_mode_default() {
        let mode = Mode::default();
        assert_eq!(mode, Mode::Passthrough);
    }

    #[test]
    fn test_mode_serialization() {
        let passthrough = Mode::Passthrough;
        let capture = Mode::Capture;

        let passthrough_json = serde_json::to_string(&passthrough).unwrap();
        let capture_json = serde_json::to_string(&capture).unwrap();

        assert_eq!(passthrough_json, "\"passthrough\"");
        assert_eq!(capture_json, "\"capture\"");
    }

    #[test]
    fn test_mode_deserialization() {
        let passthrough: Mode = serde_json::from_str("\"passthrough\"").unwrap();
        let capture: Mode = serde_json::from_str("\"capture\"").unwrap();

        assert_eq!(passthrough, Mode::Passthrough);
        assert_eq!(capture, Mode::Capture);
    }
}
