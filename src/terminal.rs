use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TerminalError {
    #[error("failed to enable raw mode: {0}")]
    EnableRawMode(#[source] std::io::Error),
}

/// RAII guard for terminal raw mode.
///
/// When created, enables raw mode on the terminal. When dropped (even on panic),
/// restores the terminal to its previous state.
///
/// Raw mode is needed to capture all keystrokes (including Ctrl+C, etc.) and
/// forward them to the PTY instead of having the local terminal handle them.
pub struct RawModeGuard {
    _private: (),
}

impl RawModeGuard {
    pub fn new() -> Result<Self, TerminalError> {
        enable_raw_mode().map_err(TerminalError::EnableRawMode)?;
        Ok(Self { _private: () })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}
