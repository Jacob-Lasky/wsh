//! End-to-end test for the full wsh data flow.
//!
//! This test verifies that input sent via async channels (like from the HTTP API)
//! actually reaches the PTY and produces output. This matches the exact architecture
//! used in main.rs where the PTY writer uses blocking_recv() in spawn_blocking.

use bytes::Bytes;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use wsh::{broker::Broker, pty::{Pty, SpawnCommand}};

/// This test replicates the exact architecture from main.rs to verify
/// that async sends from the API properly reach the blocking PTY writer.
#[tokio::test(flavor = "multi_thread")]
async fn test_async_api_input_reaches_pty() {
    // Spawn PTY
    let pty = Pty::spawn(24, 80, SpawnCommand::default()).expect("Failed to spawn PTY");
    let mut pty_reader = pty.take_reader().expect("Failed to get reader");
    let mut pty_writer = pty.take_writer().expect("Failed to get writer");

    let broker = Broker::new();
    let broker_clone = broker.clone();

    let (input_tx, mut input_rx) = mpsc::channel::<Bytes>(64);

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_reader = stop_flag.clone();

    // PTY reader task
    let pty_reader_handle = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        while !stop_flag_reader.load(Ordering::Relaxed) {
            match pty_reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    broker_clone.publish(Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => {
                    if e.raw_os_error() != Some(5) {
                        eprintln!("PTY read error: {:?}", e);
                    }
                    break;
                }
            }
        }
    });

    // PTY writer task (EXACTLY like main.rs)
    let pty_writer_handle = tokio::task::spawn_blocking(move || {
        while let Some(data) = input_rx.blocking_recv() {
            if pty_writer.write_all(&data).is_err() {
                break;
            }
            let _ = pty_writer.flush();
        }
    });

    // Give PTY time to start shell
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut rx = broker.subscribe();

    // Simulate HTTP API input (async send, exactly like api.rs)
    let marker = "E2E_API_TEST_99999";
    let cmd = format!("echo {}\n", marker);

    // This is EXACTLY what the HTTP API does
    input_tx.send(Bytes::from(cmd)).await.expect("Failed to send to channel");

    // Collect output from broker with timeout
    let mut collected = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }

        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(data) => {
                        collected.extend_from_slice(&data);
                        if String::from_utf8_lossy(&collected).contains(marker) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    let output = String::from_utf8_lossy(&collected);

    // Clean up: send exit and wait for tasks
    let _ = input_tx.send(Bytes::from("exit\n")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    stop_flag.store(true, Ordering::Relaxed);
    drop(input_tx);

    tokio::select! {
        _ = pty_writer_handle => {}
        _ = tokio::time::sleep(Duration::from_millis(500)) => {}
    }
    tokio::select! {
        _ = pty_reader_handle => {}
        _ = tokio::time::sleep(Duration::from_millis(500)) => {}
    }

    assert!(
        output.contains(marker),
        "Expected output to contain '{}', but got:\n{}",
        marker,
        output
    );
}
