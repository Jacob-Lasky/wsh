# Reliability Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all 13 reliability issues identified in the comprehensive code review — eliminating panics, lock-ups, zombie processes, stale state, and silent failures that could wreak havoc on AI agent interactions.

**Architecture:** Each fix is surgically scoped to the affected code paths. No new crates or major refactors. We add a dedicated `mpsc` channel for the parser (replacing broadcast), add `insert_and_get` / `remove_and_detach` methods to `SessionRegistry` to eliminate TOCTOU races, make `Frame::read_from` cancellation-safe, and convert all `std::sync::RwLock` to `parking_lot::RwLock`. We also fix the server startup to propagate bind failures, add a default WS quiesce deadline, and add cleanup on session kill and server shutdown.

**Tech Stack:** Rust, tokio, parking_lot, axum, bytes

**Test command:** `nix develop -c sh -c "cargo test"`
**Check command:** `nix develop -c sh -c "cargo check"`

---

## Task 1: Give the Parser a Dedicated mpsc Channel (Issue #1 — CRITICAL)

The parser is the single source of truth for terminal state. Today it subscribes to the same `broadcast` channel as disposable streaming clients. When it lags (capacity 64), bytes are silently dropped and the VT state machine permanently diverges.

**Fix:** Give the parser its own `mpsc::Sender` held by the PTY reader. The PTY reader publishes to both the broadcast (for streaming clients) and the dedicated mpsc (for the parser). The mpsc is unbounded (`mpsc::unbounded_channel`) so the parser never drops data — backpressure is absorbed in memory, which is acceptable because the parser processes faster than the PTY produces in steady state.

**Files:**
- Modify: `src/broker.rs` — add `parser_tx: mpsc::UnboundedSender<Bytes>` and `subscribe_parser()` method
- Modify: `src/parser/mod.rs:37-51` — change `spawn()` to accept `mpsc::UnboundedReceiver<Bytes>` instead of `broadcast::Receiver<Bytes>`
- Modify: `src/parser/task.rs:10-12,29-31,93-98` — change `raw_rx` from broadcast to mpsc, remove `Lagged` handling
- Modify: `src/session.rs:158-159` — pass parser channel from broker
- Test: `src/broker.rs` (unit), `src/parser/mod.rs` (unit)

### Step 1: Write tests for the new broker parser channel

Add a unit test to `src/broker.rs` that verifies the parser channel receives data separately from broadcast subscribers:

```rust
#[tokio::test]
async fn test_parser_channel_receives_independently() {
    let broker = Broker::new();
    let mut parser_rx = broker.subscribe_parser();
    let mut broadcast_rx = broker.subscribe();

    broker.publish(Bytes::from("hello"));

    // Both should receive
    let parser_msg = parser_rx.recv().await.expect("parser should receive");
    assert_eq!(parser_msg, Bytes::from("hello"));

    let broadcast_msg = broadcast_rx.recv().await.expect("broadcast should receive");
    assert_eq!(broadcast_msg, Bytes::from("hello"));
}

#[tokio::test]
async fn test_parser_channel_does_not_lag() {
    let broker = Broker::new();
    let mut parser_rx = broker.subscribe_parser();

    // Publish more than BROADCAST_CAPACITY messages
    for i in 0..200 {
        broker.publish(Bytes::from(format!("msg-{i}")));
    }

    // Parser should receive ALL messages (unbounded)
    for i in 0..200 {
        let msg = parser_rx.recv().await.expect("parser should not lose data");
        assert_eq!(msg, Bytes::from(format!("msg-{i}")));
    }
}
```

### Step 2: Run tests to verify they fail

Run: `nix develop -c sh -c "cargo test broker::tests::test_parser_channel -q"`
Expected: compile error (method doesn't exist yet)

### Step 3: Implement the parser channel in Broker

Modify `src/broker.rs`:
- Add `parser_tx: mpsc::UnboundedSender<Bytes>` and `parser_rx: Mutex<Option<mpsc::UnboundedReceiver<Bytes>>>` fields to `Broker`
- In `new()`, create `mpsc::unbounded_channel()` alongside the broadcast channel
- In `publish()`, send to both: `let _ = self.tx.send(data.clone()); let _ = self.parser_tx.send(data);`
- Add `subscribe_parser(&self) -> mpsc::UnboundedReceiver<Bytes>` that takes the receiver out of the `Mutex<Option<...>>` (panics if called twice — parser is singular)

```rust
use bytes::Bytes;
use std::sync::Mutex;
use tokio::sync::{broadcast, mpsc};

pub const BROADCAST_CAPACITY: usize = 64;

#[derive(Clone)]
pub struct Broker {
    tx: broadcast::Sender<Bytes>,
    parser_tx: mpsc::UnboundedSender<Bytes>,
    parser_rx: std::sync::Arc<Mutex<Option<mpsc::UnboundedReceiver<Bytes>>>>,
}

impl Broker {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (parser_tx, parser_rx) = mpsc::unbounded_channel();
        Self {
            tx,
            parser_tx,
            parser_rx: std::sync::Arc::new(Mutex::new(Some(parser_rx))),
        }
    }

    pub fn publish(&self, data: Bytes) {
        let _ = self.parser_tx.send(data.clone());
        // Ignore error - means no broadcast receivers
        let _ = self.tx.send(data);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.tx.subscribe()
    }

    pub fn sender(&self) -> broadcast::Sender<Bytes> {
        self.tx.clone()
    }

    /// Take the dedicated parser receiver. Panics if called more than once.
    pub fn subscribe_parser(&self) -> mpsc::UnboundedReceiver<Bytes> {
        self.parser_rx
            .lock()
            .expect("parser_rx mutex poisoned")
            .take()
            .expect("subscribe_parser called more than once")
    }
}
```

### Step 4: Update Parser::spawn to use mpsc

Modify `src/parser/mod.rs:37-51`:

```rust
pub fn spawn(raw_broker: &Broker, cols: usize, rows: usize, scrollback_limit: usize) -> Self {
    let (query_tx, query_rx) = mpsc::channel(32);
    let (event_tx, _) = broadcast::channel(256);

    let raw_rx = raw_broker.subscribe_parser();
    let event_tx_clone = event_tx.clone();

    tokio::spawn(task::run(
        raw_rx,
        query_rx,
        event_tx_clone,
        cols,
        rows,
        scrollback_limit,
    ));

    Self { query_tx, event_tx }
}
```

### Step 5: Update parser task to use mpsc receiver

Modify `src/parser/task.rs`:
- Change `raw_rx` parameter type from `broadcast::Receiver<Bytes>` to `mpsc::UnboundedReceiver<Bytes>`
- Remove the `Lagged` match arm
- Change `Err(broadcast::error::RecvError::Closed) => break` to `None => break` (mpsc returns `Option`)

```rust
pub async fn run(
    mut raw_rx: mpsc::UnboundedReceiver<Bytes>,
    mut query_rx: mpsc::Receiver<(Query, oneshot::Sender<QueryResponse>)>,
    event_tx: broadcast::Sender<Event>,
    cols: usize,
    rows: usize,
    scrollback_limit: usize,
) {
    // ... setup unchanged ...
    loop {
        tokio::select! {
            result = raw_rx.recv() => {
                match result {
                    Some(bytes) => {
                        // ... existing processing unchanged ...
                    }
                    None => break,  // Channel closed
                }
            }
            // ... query_rx branch unchanged ...
        }
    }
}
```

Remove unused `broadcast` import if no longer needed in `task.rs`.

### Step 6: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 7: Commit

```bash
git add src/broker.rs src/parser/mod.rs src/parser/task.rs
git commit -m "fix: give parser a dedicated mpsc channel to prevent data loss on lag"
```

---

## Task 2: Eliminate TOCTOU Panics in Session Create/Rename (Issue #2)

Three call sites do `.expect("just inserted session")` after calling `registry.insert()` followed by `monitor_child_exit()`. The monitor task can race and remove the session before the `.get()` call, causing a panic.

**Fix:** Add `insert_and_get()` to `SessionRegistry` that returns the `Session` clone atomically (under the write lock). Build `SessionInfo` from the returned clone instead of doing a separate `.get()`. For rename, similarly return the session from `rename()`.

**Files:**
- Modify: `src/session.rs:313-349,376-396` — add `insert_and_get`, change `rename` return type
- Modify: `src/api/handlers.rs:1913-1953,1963-1976` — use new methods
- Modify: `src/mcp/mod.rs:158-185` — use new method
- Test: `src/session.rs` (unit)

### Step 1: Write test for insert_and_get

In `src/session.rs` unit tests, add:

```rust
#[tokio::test]
async fn test_insert_and_get_returns_session() {
    let registry = SessionRegistry::new();
    let session = create_test_session("test");
    let (name, returned) = registry
        .insert_and_get(Some("test".into()), session)
        .unwrap();
    assert_eq!(name, "test");
    assert_eq!(returned.name, "test");
}
```

### Step 2: Run test to verify it fails

Run: `nix develop -c sh -c "cargo test session::tests::test_insert_and_get -q"`
Expected: compile error

### Step 3: Add `insert_and_get` to SessionRegistry

In `src/session.rs`, add a new method after `insert()`:

```rust
/// Insert a session and return a clone, atomically under the write lock.
///
/// This avoids the TOCTOU race between `insert()` and a subsequent `get()`
/// where `monitor_child_exit` could remove the session in between.
pub fn insert_and_get(
    &self,
    name: Option<String>,
    mut session: Session,
) -> Result<(String, Session), RegistryError> {
    let mut inner = self.inner.write();

    let assigned_name = match name {
        Some(n) => {
            if inner.sessions.contains_key(&n) {
                return Err(RegistryError::NameExists(n));
            }
            n
        }
        None => {
            let mut id = inner.next_id;
            loop {
                let candidate = id.to_string();
                if !inner.sessions.contains_key(&candidate) {
                    inner.next_id = id + 1;
                    break candidate;
                }
                id += 1;
            }
        }
    };

    session.name = assigned_name.clone();
    let cloned = session.clone();
    inner.sessions.insert(assigned_name.clone(), session);

    let _ = self.events_tx.send(SessionEvent::Created {
        name: assigned_name.clone(),
    });

    Ok((assigned_name, cloned))
}
```

### Step 4: Change `rename` to return the renamed Session

Modify `src/session.rs` `rename()` to return `Result<Session, RegistryError>`:

```rust
pub fn rename(&self, old_name: &str, new_name: &str) -> Result<Session, RegistryError> {
    let mut inner = self.inner.write();

    if !inner.sessions.contains_key(old_name) {
        return Err(RegistryError::NotFound(old_name.to_string()));
    }
    if inner.sessions.contains_key(new_name) {
        return Err(RegistryError::NameExists(new_name.to_string()));
    }

    let mut session = inner.sessions.remove(old_name).unwrap();
    session.name = new_name.to_string();
    let cloned = session.clone();
    inner.sessions.insert(new_name.to_string(), session);

    let _ = self.events_tx.send(SessionEvent::Renamed {
        old_name: old_name.to_string(),
        new_name: new_name.to_string(),
    });

    Ok(cloned)
}
```

### Step 5: Update all call sites

**`src/api/handlers.rs` session_create (~line 1936-1952):**

Replace:
```rust
let assigned_name = state
    .sessions
    .insert(req.name, session)
    .map_err(|e| match e { ... })?;

state.sessions.monitor_child_exit(assigned_name.clone(), child_exit_rx);

let session = state.sessions.get(&assigned_name)
    .expect("just inserted session");
Ok((
    StatusCode::CREATED,
    Json(build_session_info(&session)),
))
```

With:
```rust
let (assigned_name, session) = state
    .sessions
    .insert_and_get(req.name, session)
    .map_err(|e| match e {
        RegistryError::NameExists(n) => ApiError::SessionNameConflict(n),
        RegistryError::NotFound(n) => ApiError::SessionNotFound(n),
    })?;

state.sessions.monitor_child_exit(assigned_name.clone(), child_exit_rx);

Ok((
    StatusCode::CREATED,
    Json(build_session_info(&session)),
))
```

**`src/api/handlers.rs` session_rename (~line 1968-1975):**

Replace:
```rust
state.sessions.rename(&name, &req.name).map_err(|e| match e { ... })?;

let session = state.sessions.get(&req.name)
    .expect("just renamed session");
Ok(Json(build_session_info(&session)))
```

With:
```rust
let session = state.sessions.rename(&name, &req.name).map_err(|e| match e {
    RegistryError::NameExists(n) => ApiError::SessionNameConflict(n),
    RegistryError::NotFound(n) => ApiError::SessionNotFound(n),
})?;

Ok(Json(build_session_info(&session)))
```

**`src/mcp/mod.rs` wsh_create_session (~line 158-185):**

Same pattern — use `insert_and_get`, build JSON from the returned session clone.

### Step 6: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 7: Commit

```bash
git add src/session.rs src/api/handlers.rs src/mcp/mod.rs
git commit -m "fix: eliminate TOCTOU panics in session create/rename with atomic insert_and_get"
```

---

## Task 3: Propagate HTTP Bind Failures (Issue #3)

`TcpListener::bind().await.unwrap()` inside `tokio::spawn` silently kills the HTTP server if the port is in use. The "listening" log fires before the bind.

**Fix:** Bind the listener *before* spawning the task. Pass the bound `TcpListener` into the spawned task. Log "listening" after the bind succeeds.

**Files:**
- Modify: `src/main.rs:297-311` — move bind out of spawn, propagate error

### Step 1: Write test (manual verification)

This is a startup path — we'll verify via `cargo check` and manual testing. No unit test needed.

### Step 2: Refactor the HTTP server spawn

In `src/main.rs`, replace lines ~297-311:

```rust
// Bind the HTTP listener eagerly so errors propagate to the caller.
let listener = tokio::net::TcpListener::bind(bind)
    .await
    .map_err(|e| WshError::Io(e))?;
tracing::info!(addr = %bind, "HTTP/WS server listening");

let http_handle = tokio::spawn(async move {
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            server_shutdown_rx.await.ok();
        })
        .await
    {
        tracing::error!(?e, "HTTP server error");
    }
});
```

### Step 3: Run check + tests

Run: `nix develop -c sh -c "cargo check && cargo test -q"`
Expected: all pass

### Step 4: Commit

```bash
git add src/main.rs
git commit -m "fix: propagate HTTP bind failure instead of panicking inside spawned task"
```

---

## Task 4: Default WebSocket `await_quiesce` Deadline (Issue #4)

The WS `await_quiesce` path has no default for `max_wait_ms`, unlike the HTTP (30s) and MCP (30s) handlers. An agent can hang forever.

**Fix:** Default `max_wait_ms` in `AwaitQuiesceParams` to 30000 using `serde(default)`.

**Files:**
- Modify: `src/api/ws_methods.rs` — add `#[serde(default = "...")]` to `max_wait_ms`
- Modify: `src/api/handlers.rs:441-462` — simplify: always wrap in timeout since `max_wait_ms` always has a value
- Test: `tests/ws_json_methods.rs` or `tests/quiesce_integration.rs`

### Step 1: Write test for default deadline

Add to `tests/quiesce_integration.rs` or `tests/ws_json_methods.rs`:

```rust
#[tokio::test]
async fn ws_await_quiesce_has_default_deadline() {
    // Send await_quiesce without max_wait_ms, verify it times out
    // rather than hanging forever. Use a very short timeout_ms so
    // the session is never quiescent (run a command first).
    // Expect a timeout response within ~30s (but for the test we
    // just verify the params deserialize with a default).
}
```

Actually, the simplest verification is a unit test on deserialization:

In `src/api/ws_methods.rs` tests:
```rust
#[test]
fn await_quiesce_params_defaults_max_wait() {
    let json = r#"{"timeout_ms": 500}"#;
    let params: AwaitQuiesceParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.max_wait_ms, 30_000);
}
```

### Step 2: Run test to verify it fails

Expected: fails (currently `max_wait_ms` is `Option<u64>`, no default)

### Step 3: Change `max_wait_ms` from `Option<u64>` to `u64` with default

In `src/api/ws_methods.rs`, modify `AwaitQuiesceParams`:

```rust
fn default_ws_max_wait() -> u64 {
    30_000
}

#[derive(Debug, Deserialize)]
pub struct AwaitQuiesceParams {
    pub timeout_ms: u64,
    #[serde(default)]
    pub format: Format,
    #[serde(default = "default_ws_max_wait")]
    pub max_wait_ms: u64,
    pub last_generation: Option<u64>,
    #[serde(default)]
    pub fresh: bool,
}
```

### Step 4: Simplify the handler in `src/api/handlers.rs`

Replace the `if let Some(max_wait) ... else ...` block (~lines 441-462) with a single path:

```rust
let deadline = std::time::Duration::from_millis(params.max_wait_ms);
let fut: std::pin::Pin<Box<dyn std::future::Future<Output = Option<u64>> + Send>> =
    Box::pin(async move {
        let inner = if fresh {
            futures::future::Either::Left(activity.wait_for_fresh_quiescence(timeout))
        } else {
            futures::future::Either::Right(activity.wait_for_quiescence(timeout, last_generation))
        };
        tokio::time::timeout(deadline, inner)
            .await
            .ok()
    });
```

### Step 5: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 6: Commit

```bash
git add src/api/ws_methods.rs src/api/handlers.rs
git commit -m "fix: default WS await_quiesce max_wait_ms to 30s to prevent agent stalls"
```

---

## Task 5: Detach Streaming Clients on Session Kill (Issue #6)

`session_kill` (HTTP) and `handle_kill_session` (socket) call `registry.remove()` without calling `session.detach()` first. Streaming clients can hang forever.

**Fix:** Call `session.detach()` before removing, matching what `monitor_child_exit` already does.

**Files:**
- Modify: `src/api/handlers.rs:1978-1987` — detach before remove
- Modify: `src/server.rs:331-360` — detach before remove
- Test: existing `tests/graceful_shutdown.rs` or `tests/server_client_e2e.rs` should cover this

### Step 1: Fix HTTP session_kill

Replace the handler body:

```rust
pub(super) async fn session_kill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let session = state
        .sessions
        .remove(&name)
        .ok_or_else(|| ApiError::SessionNotFound(name))?;
    session.detach();
    Ok(StatusCode::NO_CONTENT)
}
```

### Step 2: Fix socket handle_kill_session

In `src/server.rs`, in the `Some(_)` arm of `handle_kill_session`, detach the session:

```rust
Some(session) => {
    session.detach();
    tracing::info!(session = %msg.name, "session killed via socket");
    // ... rest unchanged
}
```

### Step 3: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 4: Commit

```bash
git add src/api/handlers.rs src/server.rs
git commit -m "fix: detach streaming clients before removing session on kill"
```

---

## Task 6: Make Frame::read_from Cancellation-Safe (Issue #8)

`Frame::read_from` does three sequential reads and is used inside `select!` loops. If cancelled between reads, the protocol stream is corrupted.

**Fix:** Read the 5-byte header in a single `read_exact` call, then read the payload. This makes the first await point atomic (either no bytes consumed or all 5 consumed).

**Files:**
- Modify: `src/protocol.rs:112-136` — change `read_from` to read header atomically

### Step 1: Verify existing tests pass

Run: `nix develop -c sh -c "cargo test protocol::tests -q"`
Expected: all pass

### Step 2: Rewrite `read_from` for cancellation safety

```rust
/// Read a frame from an async reader.
///
/// The 5-byte header (type + length) is read in a single `read_exact`
/// call so that this future is cancellation-safe in `select!` loops:
/// either the header is fully consumed or no bytes are consumed.
pub async fn read_from<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<Self> {
    let mut header = [0u8; 5];
    reader.read_exact(&mut header).await?;

    let type_byte = header[0];
    let frame_type = FrameType::from_u8(type_byte).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown frame type: 0x{:02x}", type_byte),
        )
    })?;

    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]);
    if length > MAX_PAYLOAD_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame payload too large: {} bytes", length),
        ));
    }

    let mut payload = vec![0u8; length as usize];
    reader.read_exact(&mut payload).await?;

    Ok(Self {
        frame_type,
        payload: Bytes::from(payload),
    })
}
```

**Note:** The payload `read_exact` is still a separate await point, but once we've consumed the header, the subsequent `read_exact` is non-lossy — if cancelled, the reader is at a known position (middle of payload) and the connection will error on next read attempt, which is the correct behavior (connection is dead, not silently corrupted).

### Step 3: Run protocol tests

Run: `nix develop -c sh -c "cargo test protocol::tests -q"`
Expected: all pass (round-trip tests validate correctness)

### Step 4: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 5: Commit

```bash
git add src/protocol.rs
git commit -m "fix: make Frame::read_from cancellation-safe with atomic header read"
```

---

## Task 7: Switch std::sync::RwLock to parking_lot::RwLock (Issue #12)

Four modules use `std::sync::RwLock` with `.unwrap()` on every lock acquisition. A poison cascade from any panic would take down per-session overlay/panel/input systems. `parking_lot::RwLock` doesn't poison.

**Files:**
- Modify: `src/overlay/store.rs` — change import, remove all `.unwrap()` from lock calls
- Modify: `src/panel/store.rs` — same
- Modify: `src/input/mode.rs` — same
- Modify: `src/input/focus.rs` — same

### Step 1: Update `src/overlay/store.rs`

Change:
```rust
use std::sync::{Arc, RwLock};
```
To:
```rust
use std::sync::Arc;
use parking_lot::RwLock;
```

Then remove every `.unwrap()` after `.read()` and `.write()` calls throughout the file. `parking_lot::RwLock::read()` returns `RwLockReadGuard` directly (no `Result`). Same for `.write()`.

### Step 2: Update `src/panel/store.rs`

Same change as step 1.

### Step 3: Update `src/input/mode.rs`

Change:
```rust
use std::sync::{Arc, RwLock};
```
To:
```rust
use std::sync::Arc;
use parking_lot::RwLock;
```
Remove `.unwrap()` from all `.read()` and `.write()` calls.

### Step 4: Update `src/input/focus.rs`

Same change.

### Step 5: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 6: Commit

```bash
git add src/overlay/store.rs src/panel/store.rs src/input/mode.rs src/input/focus.rs
git commit -m "fix: replace std::sync::RwLock with parking_lot::RwLock to prevent poison cascades"
```

---

## Task 8: Detach + Kill All Sessions on Server Shutdown (Issue #9)

On Ctrl+C, the server exits without explicitly terminating session child processes. Children die lazily when PTY FDs close — but shells that trap SIGHUP can survive.

**Fix:** Add a `drain` method to `SessionRegistry` that detaches and removes all sessions. Call it during server shutdown.

**Files:**
- Modify: `src/session.rs` — add `drain()` method
- Modify: `src/main.rs:369-391` — call `sessions.drain()` during shutdown
- Test: `src/session.rs` (unit)

### Step 1: Add `drain` to SessionRegistry

```rust
/// Remove all sessions, detaching streaming clients first.
///
/// Called during server shutdown to ensure child processes are cleaned up
/// promptly (dropping the Session closes PTY handles, which sends SIGHUP
/// to the child).
pub fn drain(&self) {
    let names = self.list();
    for name in names {
        if let Some(session) = self.remove(&name) {
            session.detach();
        }
    }
}
```

### Step 2: Call drain during shutdown

In `src/main.rs`, after `shutdown.shutdown()` and the 100ms sleep, before sending `server_shutdown_tx`:

```rust
// Signal WebSocket handlers to send close frames
shutdown.shutdown();
// Give handlers a moment to flush close frames before stopping the server
tokio::time::sleep(std::time::Duration::from_millis(100)).await;

// Detach all streaming clients and clean up sessions. Dropping
// sessions closes PTY handles, which sends SIGHUP to children.
sessions.drain();

let _ = server_shutdown_tx.send(());
```

### Step 3: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 4: Commit

```bash
git add src/session.rs src/main.rs
git commit -m "fix: drain all sessions on server shutdown to clean up child processes"
```

---

## Task 9: Check Name Before Spawning PTY (Issue #10)

Both `session_create` (HTTP) and `handle_create_session` (socket) spawn a full PTY before checking if the session name is available. On conflict, the PTY and child are created and immediately torn down.

**Fix:** Add a `name_available` method to `SessionRegistry`. Check it before spawning.

**Files:**
- Modify: `src/session.rs` — add `name_available()` method
- Modify: `src/api/handlers.rs:1913-1942` — check name first
- Modify: `src/server.rs:145-170` — check name first
- Modify: `src/mcp/mod.rs` — check name first
- Test: `src/session.rs` (unit)

### Step 1: Add `name_available` to SessionRegistry

```rust
/// Check if a given name is available (not already in use).
///
/// Returns `Ok(())` if the name is `None` (auto-assign) or the name is free.
/// Returns `Err(RegistryError::NameExists)` if the name is taken.
pub fn name_available(&self, name: &Option<String>) -> Result<(), RegistryError> {
    if let Some(n) = name {
        let inner = self.inner.read();
        if inner.sessions.contains_key(n) {
            return Err(RegistryError::NameExists(n.clone()));
        }
    }
    Ok(())
}
```

### Step 2: Add pre-check in HTTP handler

In `src/api/handlers.rs` `session_create`, before the `Session::spawn_with_options` call:

```rust
// Pre-check name availability to avoid spawning a PTY that would be
// immediately discarded on name conflict. This is a TOCTOU hint (the
// name could be taken between the check and the insert), but insert()
// will catch that and we only waste a PTY in the rare race case.
state.sessions.name_available(&req.name).map_err(|e| match e {
    RegistryError::NameExists(n) => ApiError::SessionNameConflict(n),
    RegistryError::NotFound(n) => ApiError::SessionNotFound(n),
})?;
```

### Step 3: Add pre-check in socket handler and MCP handler

Same pattern in `src/server.rs` `handle_create_session` and `src/mcp/mod.rs` `wsh_create_session`.

### Step 4: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 5: Commit

```bash
git add src/session.rs src/api/handlers.rs src/server.rs src/mcp/mod.rs
git commit -m "fix: pre-check session name availability before spawning PTY"
```

---

## Task 10: Send Error for Replaced pending_quiesce (Issue #13)

If a WS client sends a second `await_quiesce` while the first is pending, the first request silently gets no response.

**Fix:** Before replacing `pending_quiesce`, send an error response for the old request.

**Files:**
- Modify: `src/api/handlers.rs:464-465` — send error before replacing

### Step 1: Send error for displaced request

At line ~464, before the assignment, add:

```rust
// If there's already a pending quiesce, cancel it with an error
// so the client doesn't hang waiting for a response.
if let Some((old_id, _, _)) = pending_quiesce.take() {
    let resp = super::ws_methods::WsResponse::error(
        old_id,
        "await_quiesce",
        "quiesce_superseded",
        "A new await_quiesce request superseded this one.",
    );
    if let Ok(json) = serde_json::to_string(&resp) {
        if ws_tx.send(Message::Text(json)).await.is_err() {
            break;
        }
    }
}
pending_quiesce = Some((req.id.clone(), format, fut));
```

### Step 2: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 3: Commit

```bash
git add src/api/handlers.rs
git commit -m "fix: send error response when a pending await_quiesce is superseded"
```

---

## Task 11: Auto-Release Input Capture on WS Disconnect (Issue #5)

If an agent captures input and then disconnects, the session is stuck in capture mode. No ownership tracking means any agent can capture and nobody auto-releases.

**Fix:** Track whether the current WS connection activated capture mode. On disconnect, if this connection captured, release. This is a lightweight approach that covers the common case (single agent) without full ownership tracking.

**Files:**
- Modify: `src/api/handlers.rs` — track capture in WS handler, release on exit

### Step 1: Track capture state in WS handler

In `handle_ws_json`, near the `pending_quiesce` local variable, add:

```rust
let mut this_connection_captured = false;
```

### Step 2: Set the flag when this connection captures

In the WS dispatch, when `input_capture` is called successfully (look for how the dispatch routes `input_capture` — it may be done via the `dispatch()` function or directly). After the capture call succeeds, set `this_connection_captured = true`. After the release call succeeds, set `this_connection_captured = false`.

This requires intercepting the `input_capture` and `input_release` methods in the WS handler. If they go through `dispatch()`, we can check the method name after dispatch returns.

After the dispatch returns, if `req.method == "input_capture"` and the response was successful:
```rust
if req.method == "input_capture" {
    this_connection_captured = true;
}
if req.method == "input_release" {
    this_connection_captured = false;
}
```

### Step 3: Release on WS exit

In the cleanup section of `handle_ws_json` (after the main loop breaks, before sending the close frame), add:

```rust
// Auto-release input capture if this connection activated it.
if this_connection_captured {
    session.input_mode.release();
    session.focus.unfocus();
    tracing::debug!("auto-released input capture on WS disconnect");
}
```

### Step 4: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 5: Commit

```bash
git add src/api/handlers.rs
git commit -m "fix: auto-release input capture when the capturing WS client disconnects"
```

---

## Task 12: Ephemeral Server Idle Timeout (Issue #11)

If the client crashes before creating a session, the ephemeral server runs forever because the shutdown monitor only triggers on `Destroyed` events.

**Fix:** Add an idle timeout to the ephemeral monitor — if no sessions are created within 30 seconds of startup, shut down.

**Files:**
- Modify: `src/main.rs:325-352` — add startup idle timeout

### Step 1: Add idle timeout to ephemeral monitor

In `src/main.rs`, modify the ephemeral shutdown monitor to also check for an initial idle timeout:

```rust
let ephemeral_handle = tokio::spawn(async move {
    if !config_for_monitor.is_persistent() {
        // Give the client 30 seconds to create its first session.
        // If nothing happens, the daemon was likely orphaned.
        let idle_timeout = tokio::time::sleep(std::time::Duration::from_secs(30));
        tokio::pin!(idle_timeout);

        // Wait for either the first event or the idle timeout
        loop {
            tokio::select! {
                result = events.recv() => {
                    match result {
                        Ok(_) => break, // Got an event, enter normal monitoring
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => break,
                    }
                }
                _ = &mut idle_timeout => {
                    if sessions_for_monitor.len() == 0 {
                        tracing::info!("no sessions created within idle timeout, ephemeral server shutting down");
                        return true;
                    }
                    break; // Sessions exist somehow, enter normal monitoring
                }
            }
        }
    }

    // Normal monitoring: wait for all sessions to end
    loop {
        match events.recv().await {
            Ok(event) => {
                let is_removal = matches!(
                    event,
                    wsh::session::SessionEvent::Destroyed { .. }
                );
                if is_removal
                    && !config_for_monitor.is_persistent()
                    && sessions_for_monitor.len() == 0
                {
                    tracing::info!(
                        "last session ended, ephemeral server shutting down"
                    );
                    return true;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }
});
```

### Step 2: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 3: Commit

```bash
git add src/main.rs
git commit -m "fix: add idle timeout to ephemeral server to prevent orphaned daemons"
```

---

## Task 13: Clean Up Stale Overlays/Panels on WS Disconnect (Issue #7)

When a WS client disconnects, overlays and panels it created remain forever. There's no ownership tracking.

**Fix:** Track overlay and panel IDs created by each WS connection. On disconnect, remove them. This is lightweight — just a `Vec<String>` per connection.

**Files:**
- Modify: `src/api/handlers.rs` — track created overlay/panel IDs, clean up on exit

### Step 1: Track created resource IDs

In `handle_ws_json`, add tracking state near `pending_quiesce`:

```rust
let mut owned_overlay_ids: Vec<String> = Vec::new();
let mut owned_panel_ids: Vec<String> = Vec::new();
```

### Step 2: Record IDs on creation

After the dispatch of methods that create overlays/panels (check `ws_methods.rs` for methods like `wsh_overlay`, `create_panel`, etc.), if the response contains an `id`, push it to the tracking vec.

Look for the overlay create path — it goes through `dispatch()` which returns a `WsResponse`. After dispatch, if the method is `wsh_overlay` or similar and the response is successful, extract the id from `resp.result`:

```rust
// After dispatch returns:
if req.method == "wsh_overlay" || req.method == "create_overlay" {
    if let Some(result) = &resp.result {
        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            owned_overlay_ids.push(id.to_string());
        }
    }
}
// Same for panels
```

The exact method names need to be verified from `ws_methods.rs`. Check which methods create overlays/panels and what field names the response uses.

### Step 3: Clean up on WS exit

After the main loop breaks, before sending the close frame:

```rust
// Remove overlays and panels created by this connection.
for id in &owned_overlay_ids {
    session.overlays.delete(id);
}
for id in &owned_panel_ids {
    session.panels.delete(id);
}
if !owned_overlay_ids.is_empty() || !owned_panel_ids.is_empty() {
    tracing::debug!(
        overlays = owned_overlay_ids.len(),
        panels = owned_panel_ids.len(),
        "cleaned up visual resources on WS disconnect"
    );
    // Notify visual update
    let _ = session.visual_update_tx.send(crate::protocol::VisualUpdate::OverlaysChanged);
    let _ = session.visual_update_tx.send(crate::protocol::VisualUpdate::PanelsChanged);
}
```

### Step 4: Handle deletion during connection lifetime

If the client explicitly deletes an overlay/panel during the connection, remove the ID from the tracking vec so we don't try to double-delete:

```rust
if req.method == "delete_overlay" || req.method == "wsh_overlay_delete" {
    if let Some(params) = &req.params {
        if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
            owned_overlay_ids.retain(|oid| oid != id);
        }
    }
}
// Same for panels
```

**Note:** The exact method names and param structures need to be verified from the dispatch table in `ws_methods.rs`. The implementer should read the dispatch function to find the correct method names.

### Step 5: Run all tests

Run: `nix develop -c sh -c "cargo test -q"`
Expected: all pass

### Step 6: Commit

```bash
git add src/api/handlers.rs
git commit -m "fix: clean up stale overlays and panels when WS client disconnects"
```

---

## Summary of All Tasks

| Task | Issue | Severity | What |
|------|-------|----------|------|
| 1 | #1 | CRITICAL | Parser dedicated mpsc channel (no more data loss) |
| 2 | #2 | HIGH | Atomic insert_and_get (no more TOCTOU panics) |
| 3 | #3 | HIGH | HTTP bind error propagation (no more silent failures) |
| 4 | #4 | HIGH | Default WS quiesce deadline (no more agent stalls) |
| 5 | #6 | MEDIUM | Detach on session kill (no more hung clients) |
| 6 | #8 | MEDIUM | Cancellation-safe Frame::read_from |
| 7 | #12 | MEDIUM | parking_lot::RwLock everywhere (no poison cascades) |
| 8 | #9 | MEDIUM | Drain sessions on shutdown (clean child cleanup) |
| 9 | #10 | MEDIUM | Pre-check name before PTY spawn |
| 10 | #13 | MEDIUM | Error response for superseded quiesce |
| 11 | #5 | HIGH | Auto-release input capture on disconnect |
| 12 | #11 | MEDIUM | Ephemeral server idle timeout |
| 13 | #7 | MEDIUM | Clean up overlays/panels on WS disconnect |

**Estimated commits:** 13 (one per task)

**Dependencies:** Task 2 depends on Task 1 (both touch `src/session.rs` and `src/broker.rs`). All other tasks are independent.
