use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::io::{Read, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PtyError {
    #[error("failed to open pty: {0}")]
    OpenPty(#[source] anyhow::Error),

    #[error("failed to spawn command: {0}")]
    SpawnCommand(#[source] anyhow::Error),

    #[error("failed to clone reader: {0}")]
    CloneReader(#[source] anyhow::Error),

    #[error("failed to take writer: {0}")]
    TakeWriter(#[source] anyhow::Error),

    #[error("failed to resize pty: {0}")]
    Resize(#[source] anyhow::Error),

    #[error("failed to wait for child: {0}")]
    Wait(#[from] std::io::Error),
}

pub struct Pty {
    pair: PtyPair,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
}

impl Pty {
    pub fn spawn(rows: u16, cols: u16) -> Result<Self, PtyError> {
        let pty_system = native_pty_system();

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(size).map_err(PtyError::OpenPty)?;

        // Use $SHELL or fall back to /bin/sh
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = CommandBuilder::new(&shell);

        // Force interactive mode - necessary for proper readline/job control
        // when the shell doesn't auto-detect the PTY as interactive
        cmd.arg("-i");

        // Set TERM for proper terminal handling
        cmd.env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()));

        let child = pair.slave.spawn_command(cmd).map_err(PtyError::SpawnCommand)?;

        Ok(Self { pair, child: Some(child) })
    }

    pub fn take_reader(&self) -> Result<Box<dyn Read + Send>, PtyError> {
        self.pair.master.try_clone_reader().map_err(PtyError::CloneReader)
    }

    pub fn take_writer(&self) -> Result<Box<dyn Write + Send>, PtyError> {
        self.pair.master.take_writer().map_err(PtyError::TakeWriter)
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), PtyError> {
        self.pair.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }).map_err(PtyError::Resize)
    }

    pub fn take_child(&mut self) -> Option<Box<dyn portable_pty::Child + Send + Sync>> {
        self.child.take()
    }

    pub fn wait(&mut self) -> Result<portable_pty::ExitStatus, PtyError> {
        match &mut self.child {
            Some(child) => Ok(child.wait()?),
            None => Err(PtyError::Wait(std::io::Error::new(
                std::io::ErrorKind::Other,
                "child already taken",
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    /// Helper to read from PTY with a timeout to avoid blocking forever.
    /// Returns the bytes read, or an empty vec if timeout occurred.
    fn read_with_timeout(
        mut reader: Box<dyn Read + Send>,
        timeout: Duration,
    ) -> Vec<u8> {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            let mut collected = Vec::new();

            // Read in a loop until we get some data or error
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        collected.extend_from_slice(&buf[..n]);
                        // Send what we have so far
                        let _ = tx.send(collected.clone());
                        // Keep reading a bit more in case there's more output
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for data with timeout
        rx.recv_timeout(timeout).unwrap_or_default()
    }

    #[test]
    fn test_spawn_creates_pty() {
        let pty = Pty::spawn(24, 80);
        assert!(pty.is_ok(), "Failed to spawn PTY: {:?}", pty.err());
    }

    #[test]
    fn test_take_reader_returns_handle() {
        let pty = Pty::spawn(24, 80).expect("Failed to spawn PTY");
        let reader = pty.take_reader();
        assert!(reader.is_ok(), "Failed to get reader: {:?}", reader.err());
    }

    #[test]
    fn test_take_writer_returns_handle() {
        let pty = Pty::spawn(24, 80).expect("Failed to spawn PTY");
        let writer = pty.take_writer();
        assert!(writer.is_ok(), "Failed to get writer: {:?}", writer.err());
    }

    #[test]
    fn test_write_and_read_roundtrip() {
        let pty = Pty::spawn(24, 80).expect("Failed to spawn PTY");
        let mut writer = pty.take_writer().expect("Failed to get writer");
        let reader = pty.take_reader().expect("Failed to get reader");

        // Write a simple echo command
        // Use a unique marker to identify our output
        let marker = "WSH_TEST_12345";
        let cmd = format!("echo {}\n", marker);
        writer.write_all(cmd.as_bytes()).expect("Write failed");
        writer.flush().expect("Flush failed");

        // Read with timeout
        let output = read_with_timeout(reader, Duration::from_secs(2));

        // Convert to string and check for our marker
        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains(marker),
            "Expected output to contain '{}', but got: {}",
            marker,
            output_str
        );
    }

    #[test]
    fn test_resize_succeeds() {
        let pty = Pty::spawn(24, 80).expect("Failed to spawn PTY");

        // Resize to different dimensions
        let result = pty.resize(40, 120);
        assert!(result.is_ok(), "Failed to resize PTY: {:?}", result.err());

        // Resize again to confirm it works multiple times
        let result = pty.resize(25, 100);
        assert!(result.is_ok(), "Failed to resize PTY second time: {:?}", result.err());
    }

    #[test]
    fn test_multiple_readers_can_be_cloned() {
        let pty = Pty::spawn(24, 80).expect("Failed to spawn PTY");

        // Should be able to clone multiple readers
        let reader1 = pty.take_reader();
        let reader2 = pty.take_reader();

        assert!(reader1.is_ok(), "Failed to get first reader");
        assert!(reader2.is_ok(), "Failed to get second reader");
    }

    #[test]
    fn test_spawn_with_various_dimensions() {
        // Test with minimum dimensions
        let pty_small = Pty::spawn(1, 1);
        assert!(pty_small.is_ok(), "Failed to spawn PTY with 1x1 dimensions");

        // Test with larger dimensions
        let pty_large = Pty::spawn(100, 200);
        assert!(pty_large.is_ok(), "Failed to spawn PTY with 100x200 dimensions");
    }
}
