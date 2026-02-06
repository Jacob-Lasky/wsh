# Line Indexing Design

**Status:** Implemented

> Enable clients to interpret line indices relative to buffer state

## Problem

When a client connects mid-session or receives a line event, they have no context for interpreting line indices:

1. **Correlation problem**: `/screen` returns lines without indices, but `Event::Line` includes an index. Clients can't map "line 1,692 changed" to their screen array.

2. **Semantic problem**: An AI agent sees `index: 1692` but doesn't know if that's near the "bottom" (recent) or somewhere in history.

The current model:
- Index 0 = oldest line in scrollback
- Index increases toward more recent output
- Visible screen is the last N lines (where N = terminal rows)

But neither WebSocket events nor `/screen` responses include total line count, so clients can't determine where an index falls relative to the buffer.

## Solution

Include `total_lines` in events and `first_line_index`/`total_lines` in screen responses. Clients can then:
- Calculate distance from end: `total_lines - index - 1`
- Map event indices to screen positions: `screen_position = index - first_line_index`
- Determine if an event is within visible range

## Changes

### Event::Line

Add `total_lines` field:

```rust
// src/parser/events.rs
pub enum Event {
    Line {
        seq: u64,
        index: usize,
        total_lines: usize,  // NEW
        line: FormattedLine,
    },
    // ... other variants unchanged
}
```

### ScreenResponse

Add `first_line_index` and `total_lines` fields:

```rust
// src/parser/state.rs
pub struct ScreenResponse {
    pub epoch: u64,
    pub first_line_index: usize,  // NEW
    pub total_lines: usize,       // NEW
    pub lines: Vec<FormattedLine>,
    pub cursor: Cursor,
    pub cols: usize,
    pub rows: usize,
    pub alternate_active: bool,
}
```

### Parser Task

Update line event emission in `src/parser/task.rs`:

```rust
let total_lines = vt.lines().count();
for line_idx in changed_lines {
    if let Some(line) = vt.lines().nth(line_idx) {
        seq += 1;
        let _ = event_tx.send(Event::Line {
            seq,
            index: line_idx,
            total_lines,
            line: format_line(line, true),
        });
    }
}
```

Update screen query handler to populate new fields.

## WebSocket JSON Format

Line event:

```json
{
  "event": "line",
  "seq": 42,
  "index": 1692,
  "total_lines": 1700,
  "line": [{"text": "$ ls", "fg": {"indexed": 7}}]
}
```

Sync event on subscription:

```json
{
  "event": "sync",
  "seq": 1,
  "screen": {
    "epoch": 0,
    "first_line_index": 1676,
    "total_lines": 1700,
    "lines": [...],
    "cursor": {"row": 23, "col": 5, "visible": true},
    "cols": 80,
    "rows": 24,
    "alternate_active": false
  },
  "scrollback_lines": 1676
}
```

## Alternate Screen Mode

When a TUI activates alternate screen mode:
- `total_lines` reflects only the alternate screen size (rows)
- `first_line_index` is 0 (alternate screen is its own isolated buffer)
- Scrollback is preserved but not reflected until alternate mode exits

```json
{
  "screen": {
    "first_line_index": 0,
    "total_lines": 24,
    "alternate_active": true
  },
  "scrollback_lines": 1676
}
```

When alternate mode exits, values snap back to reflect the full scrollback buffer.

## AI Agent Experience

1. Connect to WebSocket, subscribe
2. Receive `Sync` with `first_line_index: 1676`, `total_lines: 1700`
3. Immediately understand: "1700 lines of history, screen shows the last 24"
4. Receive `Event::Line { index: 1700, total_lines: 1701 }`
5. Know: "New line appended at the end, buffer grew by one"

The agent can always answer "how recent is this?" and "where does this fit in my current view?"

## Files to Modify

| File | Change |
|------|--------|
| `src/parser/events.rs` | Add `total_lines` to `Event::Line` |
| `src/parser/state.rs` | Add `first_line_index`, `total_lines` to `ScreenResponse` |
| `src/parser/task.rs` | Update event emission and query handlers |
| `src/api.rs` | No changes needed (uses response types) |
