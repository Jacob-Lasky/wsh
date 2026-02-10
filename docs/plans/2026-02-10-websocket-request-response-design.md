# WebSocket Request/Response Protocol Design

## Problem

The WebSocket JSON API (`/ws/json`) currently only supports subscriptions:
the client subscribes to event types, and the server pushes events. All
on-demand queries (screen state, scrollback, input mode) and mutations
(inject input, manage overlays, capture/release input) require separate
HTTP requests.

This forces clients to manage two transports — a WebSocket for real-time
events and HTTP for everything else. It adds complexity, introduces race
conditions between event delivery and state queries, and means every
WebSocket client also needs an HTTP client.

## Solution

Add a request/response protocol to the existing WebSocket JSON connection.
Clients can send method calls alongside (or instead of) subscriptions, and
the server responds over the same connection. The existing event push
mechanism is unchanged.

## Protocol Framing

### Client → Server (Requests)

```json
{
  "id": 3,
  "method": "get_screen",
  "params": {"format": "styled"}
}
```

- **`method`** (required): The method to invoke.
- **`params`** (optional): Method-specific parameters. Omit or `{}` if none.
- **`id`** (optional): Client-chosen identifier, echoed back in the
  response. Use an incrementing integer for simplicity. If omitted, the
  response will not include an `id`. Useful for clients that pipeline
  concurrent requests; unnecessary for clients that do one request at
  a time.

### Server → Client (Responses)

Successful response:

```json
{
  "id": 3,
  "method": "get_screen",
  "result": { ... }
}
```

Error response:

```json
{
  "id": 3,
  "method": "get_screen",
  "error": {"code": "parser_unavailable", "message": "Terminal parser unavailable."}
}
```

- **`method`** (always present): Echoes the request method.
- **`id`** (present only if the request included one): Echoes the request id.
- **`result`** or **`error`**: Mutually exclusive. Exactly one is present.

### Server → Client (Events, unchanged)

```json
{
  "event": "line",
  "seq": 42,
  ...
}
```

Events continue to use the `event` field. They never have `method` or `id`.

### Message Routing

A client distinguishes server messages by checking which top-level key is
present:

| Key present | Message kind |
|-------------|-------------|
| `connected` | Connection confirmation |
| `method`    | Response to a request |
| `event`     | Pushed event |

### Malformed Requests

If the server cannot parse a request (invalid JSON, missing `method`), it
responds with an error that has no `method` or `id`:

```json
{"error": {"code": "invalid_request", "message": "Missing 'method' field."}}
```

A client seeing an error with no `method` knows it was a protocol-level
issue with the request itself.

## Methods

### subscribe

Replaces the current special subscribe message. Each call replaces the
previous subscription. Triggers an immediate `sync` event so the client
has a consistent baseline.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `events` | `string[]` | required | Event types: `"lines"`, `"chars"`, `"cursor"`, `"mode"`, `"diffs"`, `"input"`, `"overlay"` |
| `interval_ms` | `integer` | `100` | Throttle interval for event delivery |
| `format` | `string` | `"styled"` | `"plain"` or `"styled"` |

**Result:**

```json
{"events": ["lines", "cursor"]}
```

### get_screen

Returns the current visible screen state.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `format` | `string` | `"styled"` | `"plain"` or `"styled"` |

**Result:** Same as `GET /screen` response body.

### get_scrollback

Returns lines from the scrollback buffer with pagination.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `format` | `string` | `"styled"` | `"plain"` or `"styled"` |
| `offset` | `integer` | `0` | Starting line offset |
| `limit` | `integer` | `100` | Maximum lines to return |

**Result:** Same as `GET /scrollback` response body.

### send_input

Injects input into the terminal PTY.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `data` | `string` | required | The input data |
| `encoding` | `string` | `"utf8"` | `"utf8"` (plain string) or `"base64"` (binary) |

For UTF-8 encoding, JSON `\uXXXX` escapes handle control characters
naturally (e.g., `"\u0003"` for Ctrl+C, `"\r"` for Enter).

**Result:** `{}` (empty object, confirms success)

### get_input_mode

Returns the current input routing mode.

**Params:** None.

**Result:**

```json
{"mode": "passthrough"}
```

### capture_input

Switches to capture mode (keyboard input goes only to API subscribers,
not to the PTY). Idempotent.

**Params:** None.

**Result:** `{}`

### release_input

Switches to passthrough mode (keyboard input goes to both API subscribers
and the PTY). Idempotent.

**Params:** None.

**Result:** `{}`

### create_overlay

Creates a new overlay at the specified position.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `x` | `integer` | required | Column position |
| `y` | `integer` | required | Row position |
| `z` | `integer` | `0` | Z-index for layering |
| `spans` | `OverlaySpan[]` | required | Styled text spans |

**Result:**

```json
{"id": "uuid-string"}
```

### list_overlays

Returns all active overlays.

**Params:** None.

**Result:**

```json
[{"id": "...", "x": 10, "y": 5, "z": 0, "spans": [...]}]
```

### get_overlay

Returns a single overlay by ID.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | required | Overlay ID |

**Result:** Overlay object.

### update_overlay

Replaces an overlay's spans, keeping position unchanged.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | required | Overlay ID |
| `spans` | `OverlaySpan[]` | required | New spans |

**Result:** `{}`

### patch_overlay

Updates an overlay's position/z-index, keeping spans unchanged.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | required | Overlay ID |
| `x` | `integer` | optional | New column position |
| `y` | `integer` | optional | New row position |
| `z` | `integer` | optional | New z-index |

**Result:** `{}`

### delete_overlay

Deletes a specific overlay.

**Params:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `string` | required | Overlay ID |

**Result:** `{}`

### clear_overlays

Removes all overlays.

**Params:** None.

**Result:** `{}`

## Error Codes

All existing HTTP error codes apply unchanged:

| Code | When |
|------|------|
| `overlay_not_found` | Overlay ID doesn't exist |
| `parser_unavailable` | Terminal parser unavailable |
| `invalid_request` | Malformed request or bad params |
| `invalid_overlay` | Invalid overlay specification |
| `invalid_format` | Invalid format parameter |
| `input_send_failed` | Failed to write to PTY |
| `internal_error` | Catch-all |

One new code:

| Code | When |
|------|------|
| `unknown_method` | Client sent an unrecognized method name |

## Connection Lifecycle

1. Client connects to `/ws/json`
2. Server sends `{"connected": true}`
3. Client sends any method at any time — no required ordering
4. If the client calls `subscribe`, events begin streaming
5. Subsequent `subscribe` calls replace the previous subscription and
   trigger a fresh `sync` event
6. Requests and events coexist independently on the connection

## Scope

### Unchanged

- **`/ws/raw`**: Raw binary WebSocket stays as-is.
- **HTTP API**: All HTTP endpoints stay as-is. No changes needed.
- **Event format**: All pushed events (`line`, `cursor`, `mode`, `reset`,
  `sync`, `diff`, `input`) keep their current shape.

### Not Included

- No WebSocket `ping`/`health` method (WebSocket-level ping/pong suffices).
- No HTTP-side changes.
- No changes to authentication (WebSocket auth continues to work as today).
