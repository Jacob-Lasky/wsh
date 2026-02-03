mod broker;
mod pty;
mod terminal;

use bytes::Bytes;
use std::io::{Read, Write};
use thiserror::Error;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Error, Debug)]
pub enum WshError {
    #[error("pty error: {0}")]
    Pty(#[from] pty::PtyError),

    #[error("terminal error: {0}")]
    Terminal(#[from] terminal::TerminalError),

    #[error("task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
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

    let pty = pty::Pty::spawn()?;
    tracing::info!("PTY spawned");

    let pty_reader = pty.take_reader()?;

    let broker = broker::Broker::new();
    let broker_clone = broker.clone();

    // PTY reader task: read from PTY, write to stdout, broadcast
    let pty_reader_handle = tokio::task::spawn_blocking(move || {
        let mut pty_reader = pty_reader;
        let mut stdout = std::io::stdout();
        let mut buf = [0u8; 4096];

        loop {
            match pty_reader.read(&mut buf) {
                Ok(0) => {
                    tracing::debug!("PTY reader: EOF");
                    break;
                }
                Ok(n) => {
                    let data = Bytes::copy_from_slice(&buf[..n]);
                    // Write to stdout
                    let _ = stdout.write_all(&data);
                    let _ = stdout.flush();
                    // Broadcast to subscribers
                    broker_clone.publish(data);
                }
                Err(e) => {
                    tracing::error!(?e, "PTY read error");
                    break;
                }
            }
        }
    });

    pty_reader_handle.await?;
    tracing::info!("PTY reader finished");

    Ok(())
}
