# WebSocket Protocol

wsh exposes two WebSocket endpoints for real-time terminal interaction.

## Raw Binary WebSocket

```
GET /ws/raw
```

A bidirectional byte stream mirroring the terminal's PTY.

### Output (server -> client)

Binary frames containing raw PTY output. This includes ANSI escape sequences,
control characters, and UTF-8 text exactly as the terminal emits them.

### Input (client -> server)

Send binary or text frames to inject bytes into the PTY. The data is forwarded
verbatim -- no JSON encoding.

### Lifecycle

1. Client sends HTTP upgrade request to `/ws/raw`
2. Connection opens; output frames begin immediately
3. Client sends input frames at any time
4. Either side closes the connection

### Use Cases

- Building custom terminal emulators
- Piping raw terminal I/O to/from external tools
- Low-overhead monitoring

---

## JSON Event WebSocket

```
GET /ws/json
```

A structured event stream with subscription-based filtering. Use this for
programmatic access to terminal state changes.

### Connection Flow

```
Client                           Server
  |                                |
  |  ---- WS upgrade ---------->  |
  |  <--- { "connected": true } - |
  |                                |
  |  ---- subscribe message ---->  |
  |  <--- { "subscribed": [...] }  |
  |  <--- sync event               |
  |                                |
  |  <--- events (continuous) ---  |
  |                                |
```

### Step 1: Connect

After the WebSocket handshake, the server sends:

```json
{"connected": true}
```

### Step 2: Subscribe

Send a subscribe message to select which event types you want:

```json
{
  "events": ["lines", "cursor", "diffs"],
  "interval_ms": 100,
  "format": "styled"
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `events` | array of strings | (required) | Event types to subscribe to |
| `interval_ms` | integer | `100` | Minimum interval between events (ms) |
| `format` | `"plain"` \| `"styled"` | `"styled"` | Line format for events containing lines |

**Available event types:**

| Type | Description |
|------|-------------|
| `lines` | Individual line updates |
| `cursor` | Cursor position changes |
| `mode` | Alternate screen enter/exit |
| `diffs` | Batched screen diffs (changed line indices + full screen) |
| `input` | Keyboard input events (requires input capture) |

The server acknowledges with:

```json
{"subscribed": ["lines", "cursor", "diffs"]}
```

### Step 3: Initial Sync

Immediately after subscribing, the server sends a `sync` event with the
complete current screen state:

```json
{
  "event": "sync",
  "seq": 0,
  "screen": {
    "epoch": 42,
    "first_line_index": 0,
    "total_lines": 24,
    "lines": ["$ "],
    "cursor": {"row": 0, "col": 2, "visible": true},
    "cols": 80,
    "rows": 24,
    "alternate_active": false
  },
  "scrollback_lines": 150
}
```

Use this to initialize your local state before processing incremental events.

### Step 4: Receive Events

Events arrive as JSON text frames. Every event has an `event` field
(discriminator) and a `seq` field (monotonically increasing sequence number).

---

## Event Types

### `line`

A single line was updated.

```json
{
  "event": "line",
  "seq": 5,
  "index": 3,
  "total_lines": 24,
  "line": [
    {"text": "$ ", "bold": true},
    {"text": "ls"}
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `index` | integer | Line number (0-based from top of visible screen) |
| `total_lines` | integer | Total lines in the terminal |
| `line` | FormattedLine | The line content (string or array of spans) |

### `cursor`

Cursor position changed.

```json
{
  "event": "cursor",
  "seq": 6,
  "row": 0,
  "col": 5,
  "visible": true
}
```

### `mode`

Terminal switched between normal and alternate screen buffer.

```json
{
  "event": "mode",
  "seq": 7,
  "alternate_active": true
}
```

When `alternate_active` is `true`, a full-screen TUI (vim, htop, etc.) is
running. When `false`, the terminal is in normal scrollback mode.

### `reset`

Terminal state was reset. Clients should re-fetch full state.

```json
{
  "event": "reset",
  "seq": 8,
  "reason": "clear_screen"
}
```

**Reset reasons:**

| Reason | Description |
|--------|-------------|
| `clear_screen` | Screen was cleared (Ctrl+L or `\e[2J`) |
| `clear_scrollback` | Scrollback buffer was cleared |
| `hard_reset` | Full terminal reset |
| `alternate_screen_enter` | Entered alternate screen buffer |
| `alternate_screen_exit` | Exited alternate screen buffer |
| `resize` | Terminal was resized |

### `sync`

Full screen state snapshot. Sent on initial connection and after resets.

```json
{
  "event": "sync",
  "seq": 9,
  "screen": { ... },
  "scrollback_lines": 150
}
```

The `screen` object has the same shape as the `GET /screen` response.

### `diff`

Batched screen update with changed line indices and full screen state.

```json
{
  "event": "diff",
  "seq": 10,
  "changed_lines": [0, 1, 23],
  "screen": { ... }
}
```

`changed_lines` lists the indices of lines that changed since the last diff.
The `screen` object contains the complete current screen.

### Input Events

When subscribed to `input` events, you receive keyboard input as it arrives.
These are broadcast from the input event system.

**Input event (keystroke):**

```json
{
  "event": "input",
  "mode": "passthrough",
  "raw": [27, 91, 65],
  "parsed": {
    "key": "ArrowUp",
    "modifiers": []
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `mode` | `"passthrough"` \| `"capture"` | Current input mode |
| `raw` | array of integers | Raw bytes of the input |
| `parsed` | object \| null | Parsed key if recognized |
| `parsed.key` | string \| null | Key name |
| `parsed.modifiers` | array of strings | Active modifiers (e.g., `["ctrl"]`) |

**Mode change event:**

```json
{
  "event": "mode",
  "mode": "capture"
}
```

Sent when the input mode changes between `passthrough` and `capture`.

## Graceful Shutdown

When wsh shuts down, it sends a WebSocket close frame with code `1000`
(normal closure) and reason `"server shutting down"` before terminating
the connection.

## Reconnection

wsh sessions are stateful on the server side but stateless on the client side.
If your WebSocket disconnects:

1. Reconnect to the same endpoint
2. The server sends `{"connected": true}` again
3. Re-send your subscribe message
4. The server sends a fresh `sync` event with current state

No state is lost. The terminal session continues regardless of client
connections.
