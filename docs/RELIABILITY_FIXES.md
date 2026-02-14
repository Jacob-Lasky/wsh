# Reliability Fixes Plan

Fixes for 16 critical/high-severity issues identified in code review.
Organized into 7 phases, ordered by dependency and impact.

---

## Phase 1: Frame Encoding Safety (Critical)

### Fix #1: Replace `assert!` with `Result` in `Frame::encode()`

**File:** `src/protocol.rs`

**Problem:** `Frame::encode()` panics via `assert!` if payload exceeds 16 MiB.
Server crashes if a large scrollback replay or overlay serialization exceeds the limit.

**Fix:**
- Change `encode()` to return `Result<Bytes, io::Error>` instead of `Bytes`.
- Change `write_to()` to propagate the error (already returns `io::Result`).
- Update all callers of `encode()` (tests, `write_to`, `Frame::decode` tests).
- Add a `Frame::control()` variant or adjust callers to handle the error.
- Add test: encoding a frame > 16 MiB returns an error instead of panicking.

---

## Phase 2: Session Kill Hardening (Zombie Prevention)

### Fix #2: Add SIGKILL escalation to per-session kill

**Files:** `src/api/handlers.rs`, `src/server.rs`, `src/mcp/mod.rs`, `src/session.rs`

**Problem:** `session_kill` (HTTP), `handle_kill_session` (socket), and MCP kill
all rely on `Arc<Pty>` eventually dropping to send SIGHUP. If any client holds a
`Session` clone, the child process stays alive indefinitely.

**Fix:**
- Add a `Session::force_kill()` method that:
  1. Calls `self.cancelled.cancel()`
  2. Calls `self.detach()`
  3. Calls `self.kill_child()` (SIGKILL immediately)
- In `session_kill` (handlers.rs), call `session.force_kill()` on the removed session.
- In `handle_kill_session` (server.rs), call `session.force_kill()`.
- In MCP `wsh_manage_session` Kill action, call `session.force_kill()` on the removed session.
- This ensures the child is killed immediately regardless of outstanding `Arc<Pty>` references.

### Fix #12: MCP kill path must call `detach()` (consistency fix)

**File:** `src/mcp/mod.rs`

**Problem:** MCP kill calls `sessions.remove()` but never calls `session.detach()`.
Socket clients attached to the killed session don't receive a clean detach signal.

**Fix:** Subsumed by Fix #2 above -- `force_kill()` calls both `cancel()` and `detach()`.

---

## Phase 3: Socket Streaming Hardening (Hang Prevention)

### Fix #6: Add `session.cancelled` to socket streaming `select!`

**File:** `src/server.rs`

**Problem:** Socket streaming loop lacks a `session.cancelled.cancelled()` branch.
When a session is killed, socket clients hang until broadcast channels close naturally.

**Fix:** Add a new branch to the `select!` in `run_streaming()`:
```rust
_ = session.cancelled.cancelled() => {
    tracing::debug!("session was killed, closing socket connection");
    let detach_frame = Frame::new(FrameType::Detach, Bytes::new());
    let _ = write_frame_with_timeout(&detach_frame, &mut writer).await;
    break;
}
```

### Fix #8: Add read timeout to `Frame::read_from` in streaming loop

**File:** `src/server.rs`

**Problem:** `Frame::read_from` in the streaming `select!` loop has no timeout.
A client sending a partial frame blocks the handler forever.

**Fix:** Wrap the `Frame::read_from` call in a timeout. Use a longer timeout than
the initial frame (e.g., 300 seconds — we expect clients to send at least
occasionally via resize/input, and the detach/cancelled branches handle cleanup).
On timeout, log and break.

### Fix #9: Make `Frame::read_from` cancellation-safe with `BufReader`

**File:** `src/server.rs`, `src/client.rs`

**Problem:** If `read_exact` for the 5-byte header is partially completed when a
competing `select!` branch wins, the next `read_from` reads from a misaligned
position.

**Fix:**
- Wrap the reader half in `tokio::io::BufReader` before entering the streaming loop.
  `BufReader` preserves buffered bytes across cancellation, so a cancelled partial
  `read_exact` will retry from the same position on the next call.
- Apply this in both `server.rs` (`run_streaming`) and `client.rs` (client streaming loop).

### Fix #7: Add per-send timeout to WebSocket sends

**File:** `src/api/handlers.rs`

**Problem:** `ws_tx.send()` calls in `handle_ws_raw`, `handle_ws_json`, and
`handle_ws_json_server` lack timeouts. A slow client blocks the handler,
preventing other `select!` branches (shutdown, cancellation, ping) from running.

**Fix:**
- Create a helper: `async fn ws_send_with_timeout(tx, msg, timeout) -> bool`
  that wraps `ws_tx.send()` in a `tokio::time::timeout(Duration::from_secs(30), ...)`.
- Replace all bare `ws_tx.send(...).await` calls inside select! loops with this helper.
- On timeout, log and break (treat as dead client).
- The close-frame sends already have 2-second timeouts -- leave those as-is.

---

## Phase 4: Silent Data Loss Prevention (Agent Safety)

### Fix #10: Notify subscribers on parser event lag

**File:** `src/parser/mod.rs`

**Problem:** `subscribe()` uses `filter_map(|r| r.ok())` which silently drops
`Lagged` errors. AI agents miss events with no notification.

**Fix:**
- Change the `subscribe()` method to return a stream that includes lag notifications.
- Create a new `SubscriptionEvent` enum:
  ```rust
  pub enum SubscriptionEvent {
      Event(Event),
      Lagged(u64),
  }
  ```
- Update `subscribe()` to map `Ok(event) => SubscriptionEvent::Event(event)` and
  `Err(Lagged(n)) => SubscriptionEvent::Lagged(n)`.
- In `handle_ws_json` (handlers.rs), when a `Lagged` event is received, send
  a JSON notification to the client: `{"type": "lagged", "skipped": n}`.
- This lets agents detect staleness and re-query screen state.

### Fix #11: Handle input event lag properly

**File:** `src/api/handlers.rs`

**Problem:** Input event `RecvError::Lagged` is silently swallowed at line 290.

**Fix:** When input lag occurs, send a JSON notification to the client:
```rust
Err(broadcast::error::RecvError::Lagged(n)) => {
    let lag_msg = serde_json::json!({"type": "input_lagged", "skipped": n});
    if let Ok(json) = serde_json::to_string(&lag_msg) {
        if ws_tx.send(Message::Text(json)).await.is_err() {
            break;
        }
    }
}
```

---

## Phase 5: Memory Safety (OOM Prevention)

### Fix #13 + #5: Bounded parser channel with dead-consumer detection

**File:** `src/broker.rs`, `src/parser/mod.rs`

**Problem:** The parser channel is unbounded. If the parser stalls or panics,
memory grows without limit. After a panic, `_raw_tx` keeps the channel alive
while the PTY reader keeps publishing.

**Fix:**
- Replace `mpsc::unbounded_channel()` with `mpsc::channel(capacity)`.
  Use a capacity of 4096 (each message is ~4 KiB = ~16 MiB max buffered).
- Change `Broker::publish()` to use `try_send()` on the parser channel.
  If the channel is full, log a warning and drop the message (the parser is
  behind, but the data is still in the broadcast channel for other consumers).
- In `Parser::spawn()`, drop `_raw_tx` when the parser task exits (move it
  into the spawned task so it's dropped on task exit). This causes subsequent
  `try_send()` calls to fail, which is a signal the parser is dead.
- Remove the `_raw_tx` field from `Parser` struct.
- The PTY reader's `broker.publish()` will silently drop data to the dead parser
  (already uses `let _ =`), which is the correct behavior.

### Fix #14: Optimize O(n) parser hot path

**File:** `src/parser/task.rs`

**Problem:** `vt.lines().count()` and `vt.lines().nth(idx)` are O(n) iterator
operations called per PTY chunk. With 10K scrollback and many changed lines,
this creates millions of iterator steps.

**Fix:**
- Cache `total_lines` by tracking it incrementally. The `avt::Changes` struct
  reports which lines changed -- we can compute total_lines from the previous
  value plus any new lines added.
- For line lookups, use `vt.line(idx)` if the `avt` API supports indexed access.
  If not, collect lines once into a Vec and index into it:
  ```rust
  let lines: Vec<_> = vt.lines().collect();
  let total_lines = lines.len();
  for line_idx in changed_lines {
      if let Some(line) = lines.get(line_idx) { ... }
  }
  ```
  This is O(n) once per chunk instead of O(n * changed_lines).

---

## Phase 6: Type Safety (Overflow Prevention)

### Fix #16: Use saturating arithmetic in overlay/panel rendering

**Files:** `src/overlay/render.rs`, `src/panel/render.rs`, `src/panel/layout.rs`

**Problem:** u16 arithmetic throughout rendering code can overflow with
attacker-controllable coordinates. Debug builds panic; release builds wrap.

**Fix:**
- `cursor_position()`: Use `row.saturating_add(1)` and `col.saturating_add(1)`.
- `render_overlay()`: Use `current_row = current_row.saturating_add(1)` for newlines.
- `overlay_line_extents()`: Use `current_width = current_width.saturating_add(...)`.
  Clamp `line.len()` to `u16::MAX` before the cast: `(line.len().min(u16::MAX as usize)) as u16`.
- `render_panel()`: Same saturating_add for `start_row + row_offset` and
  `col += seg.text.len() as u16`.
- `compute_layout()`: The subtraction at line 82 is safe because `remaining_rows`
  already tracks available space. But add a `debug_assert!` to catch invariant violations.
- Region writes: `overlay.y + write.row` and `overlay.x + write.col` use saturating_add.

---

## Phase 7: Soundness (Unsafe Code)

### Fix #15: Remove `unsafe impl Sync for Pty` — use `Mutex`

**File:** `src/pty.rs`, `src/session.rs`

**Problem:** `unsafe impl Sync for Pty` depends on `portable_pty` internals
that are not guaranteed thread-safe. `resize()` is called concurrently from
multiple tasks.

**Fix:**
- Wrap `Pty` in `parking_lot::Mutex<Pty>` instead of using `Arc<Pty>`.
- Change `Session.pty` from `Arc<Pty>` to `Arc<parking_lot::Mutex<Pty>>`.
- `resize()` calls acquire the lock briefly (microseconds).
- `take_reader()` and `take_writer()` are only called during `spawn_with_options()`
  before the `Pty` is shared, so no contention.
- Remove `unsafe impl Sync for Pty`.
- Update all callers: `session.pty.lock().resize(...)`, etc.

---

## Testing Strategy

Each phase must pass `cargo check`, `cargo test`, and `cargo clippy` before
proceeding to the next phase.

### Unit Tests to Add:
- Phase 1: Test that `Frame::encode()` returns error for oversized payload.
- Phase 2: Test that `Session::force_kill()` sends SIGKILL.
- Phase 3: Test WebSocket send timeout helper.
- Phase 4: Test that parser `subscribe()` stream yields `Lagged` events.
- Phase 5: Test bounded parser channel behavior when full.
- Phase 6: Test saturating arithmetic at u16 boundary values.

### Integration Tests:
- Session kill via HTTP/MCP/socket properly terminates child processes.
- Socket client disconnects promptly when session is killed.
- WebSocket slow-client doesn't block the server event loop.

---

## Implementation Order

```
Phase 1 (Critical)  ─── Frame safety
Phase 2 (Zombies)   ─── Session kill hardening
Phase 3 (Hangs)     ─── Socket/WS timeout hardening
Phase 4 (Data loss) ─── Lag notification
Phase 5 (Memory)    ─── Bounded channels + parser optimization
Phase 6 (Overflow)  ─── Saturating arithmetic
Phase 7 (Soundness) ─── Remove unsafe Sync
```

Phases 1-3 are the highest priority (server crashes, zombies, hangs).
Phases 4-5 affect agent reliability.
Phases 6-7 are correctness/safety improvements.
