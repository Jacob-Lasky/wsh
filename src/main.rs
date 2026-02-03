mod pty;
mod terminal;

use thiserror::Error;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Error, Debug)]
pub enum WshError {
    #[error("pty error: {0}")]
    Pty(#[from] pty::PtyError),

    #[error("terminal error: {0}")]
    Terminal(#[from] terminal::TerminalError),
}

#[tokio::main]
async fn main() -> Result<(), WshError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "wsh=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("wsh starting");

    // Enable raw mode - guard restores on drop
    let _raw_guard = terminal::RawModeGuard::new()?;

    let mut pty = pty::Pty::spawn()?;
    tracing::info!("PTY spawned");

    let status = pty.wait()?;
    tracing::info!(?status, "shell exited");

    Ok(())
}
