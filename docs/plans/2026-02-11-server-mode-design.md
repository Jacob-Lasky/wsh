# Server Mode: Multi-Session Architecture

## Overview

`wsh` adopts a client/server architecture. A long-running **server** process
owns all PTY sessions and exposes them via HTTP/WS API and Unix domain socket.
The **CLI** is a thin client that connects to the server to create, attach to,
and manage sessions.

This enables a central API endpoint for AI agents to control multiple shell
sessions, create/destroy sessions dynamically, and interact with all of them
through a single WebSocket connection.

## Server Lifecycle

### Two startup modes

**Explicit**: `wsh server` starts the daemon directly. It listens for
connections but creates no sessions. It stays alive indefinitely, even with
zero sessions. This is the mode for headless/API-driven deployments where AI
agents create sessions on demand.

**Implicit**: Running `wsh` (to create/attach a session) checks for an
existing server. If none is found, it spawns the server as a background
process, then connects to it as a client. An implicitly-started server shuts
down automatically when its last session exits.

The server tracks how it was started and uses this to determine shutdown
behavior. Explicit servers are persistent; implicit servers are ephemeral.

### Upgrade to persistent

An implicitly-started (ephemeral) server can be upgraded to persistent mode
so it remains alive after all sessions exit. Available via three interfaces:

- CLI: `wsh server persist`
- HTTP: `POST /server/persist`
- WebSocket: `{"method": "set_server_mode", "params": {"persist": true}}`

This is a one-way upgrade. There is no downgrade — if you want the server to
stop, shut it down explicitly.

### No standalone mode

There is no standalone/single-session mode. If you want isolation, run a
second server on a different socket/port. Every `wsh` instance is always a
client talking to a server.

## Session Model

A **session** is an isolated PTY with all its associated state. Everything
currently in `AppState` (minus the HTTP server) becomes per-session state:

- PTY (`Arc<Pty>`)
- Broker (broadcast sender for raw output distribution)
- Parser (terminal state machine)
- Input channel (`mpsc::Sender`)
- Overlays, panels
- Terminal size, input mode, input broadcaster
- Activity tracker
- Shutdown coordinator (per-session, for tracking clients attached to
  this session)

### Session naming

- Sessions have a single identifier: their **name**.
- Users can provide a name at creation: `wsh --name build-agent` or
  `POST /sessions` with `{"name": "build-agent"}`.
- If no name is provided, the server assigns the next available non-negative
  integer (`0`, `1`, `2`, ...).
- Names must be unique within a server. Creating a session with a duplicate
  name is an error.
- Sessions can be renamed via `PATCH /sessions/:name`.

### Session lifecycle

- Sessions are created when a CLI client runs `wsh` (creates and attaches) or
  when an API client calls `POST /sessions` (headless creation).
- Sessions are destroyed when the PTY's child process exits, or when
  explicitly killed via `DELETE /sessions/:name`.
- A session can exist with zero attached CLI clients — the PTY keeps running.
  This is essential for headless/API-driven sessions and for detach/reattach
  workflows.
- The server process owns the PTY. The CLI client never owns a PTY.

### Session creation parameters

- `name` (optional) — session name; auto-generated if omitted.
- `command` (optional) — shell or command to run; defaults to user's shell.
- `cwd` (optional) — working directory.
- `env` (optional) — environment variables.
- `size` (optional) — initial terminal dimensions (rows, cols).

## CLI Client

The CLI process is a thin proxy. It never owns a PTY.

### Responsibilities

- **Server discovery**: Check for an existing server via the Unix socket. If
  absent, spawn the server as a background process, wait for the socket to
  become available, then connect.
- **Session management**: Create a new session or attach to an existing one.
- **Terminal I/O**: Enter raw mode, forward stdin to the server, write server
  output to stdout. The local terminal emulator sees the same byte stream it
  does today.
- **Resize forwarding**: Handle SIGWINCH, send resize events to the server
  over the Unix socket. The server resizes the session's PTY.
- **Detach**: A key chord detaches the client without killing the session. The
  PTY keeps running on the server.

### Multiple clients per session

Multiple CLI clients can attach to the same session concurrently. Each
attached client gets a dedicated bounded channel (large capacity, ~100,000)
on the server side, ensuring no message loss under normal conditions. If a
client falls catastrophically behind, the server drops the connection; the
client can reconnect and resync.

### Scrollback on attach

When attaching to a session, the outer terminal starts with a blank slate —
it has no history from before the attach. The `--scrollback` flag controls
how much prior scrollback the server replays:

- `wsh attach <name>` — Default: replay entire scrollback (`--scrollback all`).
- `wsh attach --scrollback 500 <name>` — Replay the last 500 lines.
- `wsh attach --scrollback 0 <name>` — Current screen only, no scrollback.

Transfer is fast (local Unix socket, effectively memcpy speed). To avoid
screen flashing during replay, the CLI client uses **synchronized output**
(DEC private mode 2026):

1. Write `\e[?2026h` to stdout (begin synchronized update).
2. Write scrollback lines.
3. Write current screen state.
4. Write `\e[?2026l` to stdout (end synchronized update).

The outer terminal buffers everything and renders in a single frame.
Supported by alacritty, kitty, iTerm2, WezTerm, foot, and other modern
terminal emulators.

### CLI subcommands

- `wsh` — Create a new session and attach. Implicit server start if needed.
- `wsh server` — Start the server explicitly in the foreground.
- `wsh server persist` — Upgrade an implicit server to persistent mode.
- `wsh attach <name>` — Attach to an existing session.
- `wsh list` — List active sessions.
- `wsh kill <name>` — Kill a session.

These CLI subcommands are conveniences — they map to API calls.

## Communication Channels

### Unix domain socket

Used for CLI client to server communication.

- **Path**: `$XDG_RUNTIME_DIR/wsh/server.sock`, falling back to
  `/tmp/wsh-$UID/server.sock` if `XDG_RUNTIME_DIR` is unset.
- **Carries**: Session creation/attach requests, raw I/O byte streaming,
  resize events, detach signals.
- **Server discovery**: If the socket exists and accepts connections, the
  server is running.
- **Multiple servers**: A second server uses a different socket path
  (e.g., `wsh server --socket /tmp/wsh-other.sock`).

### HTTP/WS API

Used for web UI, AI agents, and external tooling.

- Binds to a configurable address (default `127.0.0.1:8080`).
- Session management endpoints at the top level.
- Per-session endpoints scoped under `/sessions/:name/`.
- Authentication (bearer token) required when binding to non-localhost,
  same as today.

## HTTP API

### Server-level endpoints

```
POST   /server/persist          — Upgrade to persistent mode
GET    /sessions                — List all sessions
POST   /sessions                — Create a new session
GET    /sessions/:name          — Session details
PATCH  /sessions/:name          — Rename session
DELETE /sessions/:name          — Kill session
```

### Per-session endpoints

All current endpoints move under `/sessions/:name/`:

```
POST   /sessions/:name/input
GET    /sessions/:name/input/mode
POST   /sessions/:name/input/capture
POST   /sessions/:name/input/release
GET    /sessions/:name/quiesce
GET    /sessions/:name/ws/raw
GET    /sessions/:name/ws/json
GET    /sessions/:name/screen
GET    /sessions/:name/scrollback
POST   /sessions/:name/overlay
GET    /sessions/:name/overlay
GET    /sessions/:name/overlay/:id
PUT    /sessions/:name/overlay/:id
DELETE /sessions/:name/overlay/:id
POST   /sessions/:name/panel
GET    /sessions/:name/panel
GET    /sessions/:name/panel/:id
PUT    /sessions/:name/panel/:id
DELETE /sessions/:name/panel/:id
```

## WebSocket API

### Server-level WebSocket

A single WebSocket at `GET /ws/json` provides multiplexed access to all
sessions. This is the primary interface for AI agents managing multiple
sessions.

Every per-session request includes a `session` field:

```json
{"method": "send_input", "session": "build-agent", "params": {"input": "ls\n"}}
{"method": "subscribe", "session": "build-agent", "params": {"events": ["output"]}}
{"method": "get_screen", "session": "frontend"}
```

Session management methods:

```json
{"method": "create_session", "params": {"name": "worker-3", "command": "bash"}}
{"method": "list_sessions"}
{"method": "kill_session", "params": {"name": "worker-3"}}
{"method": "rename_session", "params": {"name": "worker-3", "new_name": "builder"}}
{"method": "set_server_mode", "params": {"persist": true}}
```

Events include their source session:

```json
{"event": "output", "session": "build-agent", "params": {"data": "..."}}
```

Server-level events are broadcast to all connected WebSocket clients
automatically (no subscription needed):

```json
{"event": "session_created", "params": {"name": "worker-3"}}
{"event": "session_exited", "params": {"name": "worker-3"}}
{"event": "session_destroyed", "params": {"name": "worker-3"}}
```

### Per-session WebSockets

`/sessions/:name/ws/json` and `/sessions/:name/ws/raw` remain available.
They behave exactly as today — no `session` field needed, all methods and
events are implicitly scoped to that session. Useful for simple single-session
clients like a web UI viewing one session.

## Unix Socket Protocol

The Unix socket carries CLI client to server communication using a simple
length-prefixed binary protocol.

### Frame format

```
[type: u8] [length: u32] [payload: bytes]
```

### Frame types

**Control frames** (JSON-encoded payloads):

- Create session request/response
- Attach to session request/response
- Detach
- Resize (rows, cols)
- Error

**Data frames** (raw bytes):

- PTY output: server to client
- Stdin input: client to server

Minimal overhead — just the type byte and length prefix around raw bytes.

### Connection lifecycle

1. Client connects to Unix socket.
2. Client sends a control frame: "create session" (with optional name,
   command, size) or "attach to session" (with name).
3. Server responds with success (session name, initial terminal state) or
   error.
4. Connection enters streaming mode: data frames flow bidirectionally.
   Control frames (resize, detach) can be interleaved.
5. On detach or disconnect, the server removes the client from the session's
   client list. The session continues running.

This protocol is internal — not a public API. External consumers use HTTP/WS.
The protocol can evolve freely without versioning concerns since the CLI and
server are always the same binary.

## Internal Architecture

### Server process structure

The server process contains:

- **Session registry** — a concurrent map of session name to `Session`.
  Handles creation, lookup, removal, and rename.
- **HTTP/WS API server** (Axum) — routes requests to the appropriate session
  or handles server-level operations.
- **Unix socket listener** — accepts CLI client connections and manages their
  attachment to sessions.

### AppState refactor

`AppState` becomes server-level state passed to Axum handlers:

- Session registry
- Server configuration (bind address, auth token, shutdown mode)
- Server-level shutdown coordinator

Handlers extract the session from the registry using the `:name` path
parameter, then operate on it. Per-session handler logic remains largely
unchanged.

### Session lifecycle management

- Each session spawns its own PTY reader, writer, and child monitor tasks.
- When a session's child process exits, the session is marked as exited,
  server-level events are broadcast, and the session is cleaned up.
- On an implicitly-started server, after cleanup the server checks if any
  sessions remain. If none, it shuts down.

## Web UI (Future)

The web UI will have URL-routed session views:

- **Landing page** (`/`) — Lists all active sessions. Selecting a session
  navigates to its URL.
- **Session view** (`/sessions/:name`) — Full-screen terminal view for a
  specific session. Can be accessed directly via URL without going through
  the landing page.

This is documented for future reference but is not in scope for the initial
server mode implementation. The priority is enabling API use cases.
