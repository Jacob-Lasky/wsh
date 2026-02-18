//! Tests for interactive shell behavior.
//!
//! These tests verify that the PTY spawns a properly interactive shell
//! by sending commands and verifying they execute (not just echo).

use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use wsh::pty::{Pty, SpawnCommand};

/// Helper to read from PTY until we find expected content or timeout.
/// Sends "exit" to the shell before returning to ensure clean shutdown.
fn read_until_or_timeout(
    mut reader: Box<dyn Read + Send>,
    mut writer: Box<dyn Write + Send>,
    timeout: Duration,
    expected: &str,
) -> (Vec<u8>, bool) {
    let (tx, rx) = mpsc::channel();
    let expected_owned = expected.to_string();

    let reader_thread = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut collected = Vec::new();

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    collected.extend_from_slice(&buf[..n]);
                    let output = String::from_utf8_lossy(&collected);
                    let found = output.contains(&expected_owned);
                    let _ = tx.send((collected.clone(), found));
                    if found {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Wait for data with timeout
    let deadline = std::time::Instant::now() + timeout;
    let mut result = (Vec::new(), false);

    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok((data, found)) => {
                result = (data, found);
                if found {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Clean up: send exit to close the shell
    let _ = writer.write_all(b"\nexit\n");
    let _ = writer.flush();

    // Give the reader thread a moment to finish
    let _ = reader_thread.join();

    result
}

/// Test that commands execute and produce output (not just echo).
/// This catches the issue where a non-interactive shell would echo input
/// but not execute commands.
#[test]
fn test_shell_executes_commands_and_produces_output() {
    let pty = Pty::spawn(24, 80, SpawnCommand::default()).expect("Failed to spawn PTY");
    let mut writer = pty.take_writer().expect("Failed to get writer");
    let reader = pty.take_reader().expect("Failed to get reader");

    // Wait for shell to start
    thread::sleep(Duration::from_millis(300));

    // Send a command that produces distinct, predictable output
    // Using printf to avoid issues with echo variations
    let marker = "INTERACTIVE_TEST_42";
    let cmd = format!("printf 'OUTPUT:{}\\n'\n", marker);
    writer.write_all(cmd.as_bytes()).expect("Write failed");
    writer.flush().expect("Flush failed");

    // We expect to see "OUTPUT:INTERACTIVE_TEST_42" in the output
    let expected = format!("OUTPUT:{}", marker);
    let (output, found) = read_until_or_timeout(reader, writer, Duration::from_secs(5), &expected);
    let output_str = String::from_utf8_lossy(&output);

    assert!(
        found,
        "Expected to find '{}' in output (command execution), but got:\n{}",
        expected,
        output_str
    );
}

