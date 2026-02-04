use bytes::Bytes;
use wsh::{api, broker, pty, terminal};
use std::io::{Read, Write};
use std::net::SocketAddr;
use thiserror::Error;
use tokio::sync::mpsc;
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

    let (rows, cols) = terminal::terminal_size().unwrap_or((24, 80));
    tracing::info!(rows, cols, "Terminal size");
    let mut pty = pty::Pty::spawn(rows, cols)?;
    tracing::info!("PTY spawned");

    let mut pty_reader = pty.take_reader()?;
    let mut pty_writer = pty.take_writer()?;
    let mut pty_child = pty.take_child().expect("child process");

    // Channel to signal when child process exits
    let (child_exit_tx, mut child_exit_rx) = tokio::sync::oneshot::channel::<()>();

    // Child process monitor task
    let _child_monitor_handle = tokio::task::spawn_blocking(move || {
        tracing::debug!("Child monitor task started");
        match pty_child.wait() {
            Ok(status) => {
                tracing::info!(?status, "Shell process exited");
            }
            Err(e) => {
                tracing::error!(?e, "Error waiting for shell process");
            }
        }
        let _ = child_exit_tx.send(());
        tracing::debug!("Child monitor task exiting");
    });

    let broker = broker::Broker::new();
    let broker_clone = broker.clone();

    // Channel for input from all sources -> PTY writer
    let (input_tx, mut input_rx) = mpsc::channel::<Bytes>(64);

    // PTY reader task: read from PTY, write to stdout, broadcast
    let mut pty_reader_handle = tokio::task::spawn_blocking(move || {
        tracing::debug!("PTY reader task started");
        let mut stdout = std::io::stdout();
        let mut buf = [0u8; 4096];

        loop {
            match pty_reader.read(&mut buf) {
                Ok(0) => {
                    tracing::debug!("PTY reader: EOF - shell likely exited");
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
                    tracing::error!(?e, "PTY read error - shell may have exited");
                    break;
                }
            }
        }
        tracing::debug!("PTY reader task exiting");
    });

    // PTY writer task: receive from channel, write to PTY
    let _pty_writer_handle = tokio::task::spawn_blocking(move || {
        tracing::debug!("PTY writer task started");
        while let Some(data) = input_rx.blocking_recv() {
            tracing::debug!(
                bytes = data.len(),
                data_hex = ?data.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>(),
                "PTY writer: writing to PTY"
            );
            if let Err(e) = pty_writer.write_all(&data) {
                tracing::error!(?e, "PTY write error");
                break;
            }
            let _ = pty_writer.flush();
        }
        tracing::debug!("PTY writer: channel closed, task exiting");
    });

    // Stdin reader task: read from stdin, send to PTY writer channel
    let stdin_tx = input_tx.clone();
    let _stdin_handle = tokio::task::spawn_blocking(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];

        loop {
            match stdin.read(&mut buf) {
                Ok(0) => {
                    tracing::debug!("stdin: EOF - stdin reader exiting");
                    break;
                }
                Ok(n) => {
                    let data = Bytes::copy_from_slice(&buf[..n]);
                    // Log what we're reading from stdin (useful for debugging Ctrl+D issues)
                    tracing::debug!(
                        bytes = n,
                        data_hex = ?buf[..n].iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>(),
                        "stdin: read data"
                    );
                    if stdin_tx.blocking_send(data).is_err() {
                        tracing::debug!("stdin: channel closed, exiting");
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!(?e, "stdin read error");
                    break;
                }
            }
        }
        tracing::debug!("stdin reader task exiting");
    });

    // Axum server
    let state = api::AppState {
        input_tx: input_tx.clone(),
        output_rx: broker.sender(),
    };
    let app = api::router(state);
    let addr: SocketAddr = "127.0.0.1:8080".parse().expect("valid socket address");
    tracing::info!(%addr, "API server listening");

    let _server_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    // Signal handling for graceful shutdown
    let shutdown = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        tracing::info!("Received Ctrl+C, shutting down");
    };

    // Wait for either: child process to exit, PTY reader to finish, OR shutdown signal
    tokio::select! {
        _ = &mut child_exit_rx => {
            tracing::info!("Child process exited, shutting down");
            pty_reader_handle.abort();
        }
        result = &mut pty_reader_handle => {
            match result {
                Ok(()) => tracing::info!("PTY reader finished"),
                Err(e) => tracing::error!(?e, "PTY reader task failed"),
            }
        }
        _ = shutdown => {
            tracing::info!("Shutdown signal received");
            pty_reader_handle.abort();
        }
    }

    // Note: spawn_blocking tasks (stdin reader) can't be cancelled if blocked
    // on I/O. Since stdin.read() blocks until input, we must forcefully exit
    // when the shell exits to avoid hanging.
    //
    // We must manually restore terminal state since exit() bypasses destructors.
    tracing::info!("wsh exiting");
    drop(_raw_guard);
    std::process::exit(0)
}
