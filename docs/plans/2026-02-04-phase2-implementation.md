# Phase 2: Terminal Parsing - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add terminal state tracking via `avt` crate, enabling `/screen`, `/scrollback`, and `/ws/json` endpoints for agent consumption.

**Architecture:** Parser runs as separate async task, owns `avt::Vt` exclusively, handles queries via internal channels, broadcasts events to subscribers. Consumers use `parser.query()` and `parser.subscribe()`.

**Tech Stack:** avt (terminal emulation), thiserror (errors), tokio-stream (async streams), serde (JSON serialization)

---

## Task 1: Add Parser Module Skeleton

**Files:**
- Create: `src/parser/mod.rs`
- Create: `src/parser/state.rs`
- Create: `src/parser/events.rs`
- Create: `src/parser/format.rs`
- Modify: `src/lib.rs`

**Step 1: Create parser module directory and mod.rs**

```rust
// src/parser/mod.rs
pub mod events;
pub mod format;
pub mod state;

mod task;

use bytes::Bytes;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::Stream;

use crate::broker::Broker;
use events::Event;
use state::{Format, Query, QueryResponse};

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("parser task died unexpectedly")]
    TaskDied,

    #[error("query channel full")]
    ChannelFull,

    #[error("invalid query parameters: {0}")]
    InvalidQuery(String),
}

#[derive(Clone)]
pub struct Parser {
    query_tx: mpsc::Sender<(Query, oneshot::Sender<QueryResponse>)>,
    event_tx: broadcast::Sender<Event>,
}

impl Parser {
    /// Spawn parser task, subscribing to raw byte broker
    pub fn spawn(raw_broker: &Broker, cols: usize, rows: usize, scrollback_limit: usize) -> Self {
        let (query_tx, query_rx) = mpsc::channel(32);
        let (event_tx, _) = broadcast::channel(256);

        let raw_rx = raw_broker.subscribe();
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

    /// Query current state (hides channel creation)
    pub async fn query(&self, query: Query) -> Result<QueryResponse, ParserError> {
        let (tx, rx) = oneshot::channel();
        self.query_tx
            .send((query, tx))
            .await
            .map_err(|_| ParserError::TaskDied)?;
        rx.await.map_err(|_| ParserError::TaskDied)
    }

    /// Notify parser of terminal resize
    pub async fn resize(&self, cols: usize, rows: usize) -> Result<(), ParserError> {
        self.query(Query::Resize { cols, rows }).await?;
        Ok(())
    }

    /// Subscribe to events (returns async Stream)
    pub fn subscribe(&self) -> impl Stream<Item = Event> {
        BroadcastStream::new(self.event_tx.subscribe())
            .filter_map(|result| async move { result.ok() })
    }
}
```

**Step 2: Create state.rs with data types**

```rust
// src/parser/state.rs
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Plain,
    #[default]
    Styled,
}

#[derive(Debug, Clone)]
pub enum Query {
    Screen { format: Format },
    Scrollback { format: Format, offset: usize, limit: usize },
    Cursor,
    Resize { cols: usize, rows: usize },
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum QueryResponse {
    Screen(ScreenResponse),
    Scrollback(ScrollbackResponse),
    Cursor(CursorResponse),
    Ok,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenResponse {
    pub epoch: u64,
    pub lines: Vec<FormattedLine>,
    pub cursor: Cursor,
    pub cols: usize,
    pub rows: usize,
    pub alternate_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScrollbackResponse {
    pub epoch: u64,
    pub lines: Vec<FormattedLine>,
    pub total_lines: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CursorResponse {
    pub epoch: u64,
    pub cursor: Cursor,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum FormattedLine {
    Plain(String),
    Styled(Vec<Span>),
}

#[derive(Debug, Clone, Serialize)]
pub struct Span {
    pub text: String,
    #[serde(flatten)]
    pub style: Style,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub faint: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub blink: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inverse: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Indexed(u8),
    Rgb { r: u8, g: u8, b: u8 },
}
```

**Step 3: Create events.rs with event types**

```rust
// src/parser/events.rs
use serde::{Deserialize, Serialize};

use super::state::{Cursor, FormattedLine, ScreenResponse, Style};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Line {
        seq: u64,
        index: usize,
        line: FormattedLine,
    },
    Cursor {
        seq: u64,
        row: usize,
        col: usize,
        visible: bool,
    },
    Mode {
        seq: u64,
        alternate_active: bool,
    },
    Reset {
        seq: u64,
        reason: ResetReason,
    },
    Sync {
        seq: u64,
        screen: ScreenResponse,
        scrollback_lines: usize,
    },
    Diff {
        seq: u64,
        changed_lines: Vec<usize>,
        screen: ScreenResponse,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetReason {
    ClearScreen,
    ClearScrollback,
    HardReset,
    AlternateScreenEnter,
    AlternateScreenExit,
    Resize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Subscribe {
    pub events: Vec<EventType>,
    #[serde(default = "default_interval")]
    pub interval_ms: u64,
    #[serde(default)]
    pub format: super::state::Format,
}

fn default_interval() -> u64 {
    100
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Lines,
    Chars,
    Cursor,
    Mode,
    Diffs,
}
```

**Step 4: Create format.rs with conversion utilities**

```rust
// src/parser/format.rs
use avt::{Cell, Line, Pen};

use super::state::{Color, FormattedLine, Span, Style};

/// Convert an avt Line to a FormattedLine based on format
pub fn format_line(line: &Line, styled: bool) -> FormattedLine {
    if styled {
        FormattedLine::Styled(line_to_spans(line))
    } else {
        FormattedLine::Plain(line.text())
    }
}

/// Convert an avt Line to styled spans
fn line_to_spans(line: &Line) -> Vec<Span> {
    let cells = line.cells();
    if cells.is_empty() {
        return vec![];
    }

    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;

    for cell in cells {
        let ch = cell.char();
        if ch == '\0' || cell.width() == 0 {
            continue;
        }

        let style = pen_to_style(cell.pen());

        match &current_style {
            None => {
                current_style = Some(style);
                current_text.push(ch);
            }
            Some(s) if *s == style => {
                current_text.push(ch);
            }
            Some(_) => {
                // Style changed, emit current span
                if !current_text.is_empty() {
                    spans.push(Span {
                        text: std::mem::take(&mut current_text),
                        style: current_style.take().unwrap(),
                    });
                }
                current_style = Some(style);
                current_text.push(ch);
            }
        }
    }

    // Emit final span
    if !current_text.is_empty() {
        if let Some(style) = current_style {
            spans.push(Span {
                text: current_text,
                style,
            });
        }
    }

    spans
}

fn pen_to_style(pen: &Pen) -> Style {
    Style {
        fg: pen.foreground().map(color_to_color),
        bg: pen.background().map(color_to_color),
        bold: pen.is_bold(),
        faint: pen.is_faint(),
        italic: pen.is_italic(),
        underline: pen.is_underline(),
        strikethrough: pen.is_strikethrough(),
        blink: pen.is_blink(),
        inverse: pen.is_inverse(),
    }
}

fn color_to_color(c: &avt::Color) -> Color {
    match c {
        avt::Color::Indexed(i) => Color::Indexed(*i),
        avt::Color::RGB(rgb) => Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
    }
}
```

**Step 5: Create task.rs with parser task logic (placeholder)**

```rust
// src/parser/task.rs
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::events::Event;
use super::state::{Query, QueryResponse};

pub async fn run(
    mut raw_rx: broadcast::Receiver<Bytes>,
    mut query_rx: mpsc::Receiver<(Query, oneshot::Sender<QueryResponse>)>,
    event_tx: broadcast::Sender<Event>,
    cols: usize,
    rows: usize,
    scrollback_limit: usize,
) {
    let mut vt = avt::Vt::builder()
        .size(cols, rows)
        .scrollback_limit(scrollback_limit)
        .build();

    let mut seq: u64 = 0;
    let mut epoch: u64 = 0;
    let mut last_alternate_active = false;

    loop {
        tokio::select! {
            result = raw_rx.recv() => {
                match result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        let _changes = vt.feed_str(&text);

                        // TODO: Emit events based on changes
                        // For now, just track alternate screen changes
                        let alternate_active = vt.cursor().visible; // placeholder
                        if alternate_active != last_alternate_active {
                            last_alternate_active = alternate_active;
                            // Would emit Mode event
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }

            Some((query, response_tx)) = query_rx.recv() => {
                let response = handle_query(&vt, query, epoch);
                let _ = response_tx.send(response);
            }
        }
    }
}

fn handle_query(vt: &avt::Vt, query: Query, epoch: u64) -> QueryResponse {
    use super::format::format_line;
    use super::state::*;

    match query {
        Query::Screen { format } => {
            let styled = matches!(format, Format::Styled);
            let (cols, rows) = vt.size();
            let cursor = vt.cursor();

            let lines: Vec<_> = vt.view().map(|l| format_line(l, styled)).collect();

            QueryResponse::Screen(ScreenResponse {
                epoch,
                lines,
                cursor: Cursor {
                    row: cursor.row,
                    col: cursor.col,
                    visible: cursor.visible,
                },
                cols,
                rows,
                alternate_active: false, // TODO: track this properly
            })
        }

        Query::Scrollback { format, offset, limit } => {
            let styled = matches!(format, Format::Styled);
            let all_lines: Vec<_> = vt.lines().collect();
            let (_, rows) = vt.size();

            // Scrollback is lines() minus the visible view
            let scrollback_count = all_lines.len().saturating_sub(rows);
            let scrollback_lines: Vec<_> = all_lines
                .into_iter()
                .take(scrollback_count)
                .skip(offset)
                .take(limit)
                .map(|l| format_line(l, styled))
                .collect();

            QueryResponse::Scrollback(ScrollbackResponse {
                epoch,
                lines: scrollback_lines,
                total_lines: scrollback_count,
                offset,
            })
        }

        Query::Cursor => {
            let cursor = vt.cursor();
            QueryResponse::Cursor(CursorResponse {
                epoch,
                cursor: Cursor {
                    row: cursor.row,
                    col: cursor.col,
                    visible: cursor.visible,
                },
            })
        }

        Query::Resize { cols, rows } => {
            // Note: vt is not mutable here, this needs refactoring
            // For now, return Ok
            QueryResponse::Ok
        }
    }
}
```

**Step 6: Update lib.rs to export parser module**

```rust
// src/lib.rs
pub mod api;
pub mod broker;
pub mod parser;
pub mod pty;
pub mod shutdown;
pub mod terminal;
```

**Step 7: Run cargo check**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles with possibly some warnings

**Step 8: Commit**

```bash
git add src/parser/ src/lib.rs
git commit -m "feat(parser): add parser module skeleton with data types"
```

---

## Task 2: Implement Parser Task with Proper State Tracking

**Files:**
- Modify: `src/parser/task.rs`
- Modify: `src/parser/mod.rs`

**Step 1: Refactor task.rs for mutable vt access**

```rust
// src/parser/task.rs
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::events::{Event, ResetReason};
use super::format::format_line;
use super::state::{
    Cursor, CursorResponse, Format, Query, QueryResponse, ScreenResponse, ScrollbackResponse,
};

pub async fn run(
    mut raw_rx: broadcast::Receiver<Bytes>,
    mut query_rx: mpsc::Receiver<(Query, oneshot::Sender<QueryResponse>)>,
    event_tx: broadcast::Sender<Event>,
    cols: usize,
    rows: usize,
    scrollback_limit: usize,
) {
    let mut vt = avt::Vt::builder()
        .size(cols, rows)
        .scrollback_limit(scrollback_limit)
        .build();

    let mut seq: u64 = 0;
    let mut epoch: u64 = 0;
    let mut last_cursor = vt.cursor();

    loop {
        tokio::select! {
            result = raw_rx.recv() => {
                match result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        let changes = vt.feed_str(&text);

                        // Emit line events for changed lines
                        for line_idx in changes.lines() {
                            if let Some(line) = vt.lines().nth(line_idx) {
                                seq += 1;
                                let _ = event_tx.send(Event::Line {
                                    seq,
                                    index: line_idx,
                                    line: format_line(line, true),
                                });
                            }
                        }

                        // Emit cursor event if changed
                        let cursor = vt.cursor();
                        if cursor.row != last_cursor.row
                            || cursor.col != last_cursor.col
                            || cursor.visible != last_cursor.visible
                        {
                            seq += 1;
                            let _ = event_tx.send(Event::Cursor {
                                seq,
                                row: cursor.row,
                                col: cursor.col,
                                visible: cursor.visible,
                            });
                            last_cursor = cursor;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(n, "parser lagged, some output may be lost");
                        continue;
                    }
                }
            }

            Some((query, response_tx)) = query_rx.recv() => {
                let response = handle_query(&mut vt, query, epoch, &mut seq, &event_tx);
                let _ = response_tx.send(response);
            }
        }
    }
}

fn handle_query(
    vt: &mut avt::Vt,
    query: Query,
    mut epoch: u64,
    seq: &mut u64,
    event_tx: &broadcast::Sender<Event>,
) -> QueryResponse {
    match query {
        Query::Screen { format } => {
            let styled = matches!(format, Format::Styled);
            let (cols, rows) = vt.size();
            let cursor = vt.cursor();

            let lines: Vec<_> = vt.view().map(|l| format_line(l, styled)).collect();

            QueryResponse::Screen(ScreenResponse {
                epoch,
                lines,
                cursor: Cursor {
                    row: cursor.row,
                    col: cursor.col,
                    visible: cursor.visible,
                },
                cols,
                rows,
                alternate_active: false, // avt doesn't expose this directly
            })
        }

        Query::Scrollback {
            format,
            offset,
            limit,
        } => {
            let styled = matches!(format, Format::Styled);
            let all_lines: Vec<_> = vt.lines().collect();
            let (_, rows) = vt.size();

            let scrollback_count = all_lines.len().saturating_sub(rows);
            let scrollback_lines: Vec<_> = all_lines
                .into_iter()
                .take(scrollback_count)
                .skip(offset)
                .take(limit)
                .map(|l| format_line(l, styled))
                .collect();

            QueryResponse::Scrollback(ScrollbackResponse {
                epoch,
                lines: scrollback_lines,
                total_lines: scrollback_count,
                offset,
            })
        }

        Query::Cursor => {
            let cursor = vt.cursor();
            QueryResponse::Cursor(CursorResponse {
                epoch,
                cursor: Cursor {
                    row: cursor.row,
                    col: cursor.col,
                    visible: cursor.visible,
                },
            })
        }

        Query::Resize { cols, rows } => {
            let _changes = vt.resize(cols, rows);
            *seq += 1;
            let _ = event_tx.send(Event::Reset {
                seq: *seq,
                reason: ResetReason::Resize,
            });
            QueryResponse::Ok
        }
    }
}
```

**Step 2: Run cargo check and fix any issues**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/parser/task.rs
git commit -m "feat(parser): implement parser task with event emission"
```

---

## Task 3: Add Unit Tests for Parser

**Files:**
- Create: `src/parser/tests.rs`
- Modify: `src/parser/mod.rs`

**Step 1: Create tests.rs**

```rust
// src/parser/tests.rs
use super::*;
use crate::broker::Broker;
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_parser_spawn() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    // Should be able to query immediately
    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            assert_eq!(screen.cols, 80);
            assert_eq!(screen.rows, 24);
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_query_cursor() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    let response = parser.query(Query::Cursor).await.unwrap();

    match response {
        QueryResponse::Cursor(cursor_resp) => {
            assert_eq!(cursor_resp.cursor.row, 0);
            assert_eq!(cursor_resp.cursor.col, 0);
        }
        _ => panic!("expected Cursor response"),
    }
}

#[tokio::test]
async fn test_parser_processes_input() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    // Send some text through the broker
    broker.publish(bytes::Bytes::from("Hello, World!"));

    // Give the parser time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            assert!(!screen.lines.is_empty());
            if let Some(state::FormattedLine::Plain(text)) = screen.lines.first() {
                assert!(text.contains("Hello"));
            }
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_resize() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    // Resize
    parser.resize(120, 40).await.unwrap();

    // Query screen to verify new size
    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            assert_eq!(screen.cols, 120);
            assert_eq!(screen.rows, 40);
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_scrollback() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 5, 100); // Small screen for testing

    // Send enough lines to create scrollback
    for i in 0..10 {
        broker.publish(bytes::Bytes::from(format!("Line {}\r\n", i)));
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = parser
        .query(Query::Scrollback {
            format: Format::Plain,
            offset: 0,
            limit: 100,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Scrollback(scrollback) => {
            // Should have some scrollback
            assert!(scrollback.total_lines > 0);
        }
        _ => panic!("expected Scrollback response"),
    }
}

#[tokio::test]
async fn test_parser_event_stream() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    let mut events = parser.subscribe();

    // Send text
    broker.publish(bytes::Bytes::from("Test"));

    // Should receive events
    let event = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        events.next(),
    )
    .await;

    assert!(event.is_ok(), "should receive an event");
}
```

**Step 2: Add tests module to mod.rs**

Add at the end of `src/parser/mod.rs`:

```rust
#[cfg(test)]
mod tests;
```

**Step 3: Run tests**

Run: `nix develop -c sh -c "cargo test parser"`
Expected: All parser tests pass

**Step 4: Commit**

```bash
git add src/parser/tests.rs src/parser/mod.rs
git commit -m "test(parser): add unit tests for parser module"
```

---

## Task 4: Add /screen and /scrollback HTTP Endpoints

**Files:**
- Modify: `src/api.rs`

**Step 1: Update AppState to include Parser**

Add to imports and update AppState in `src/api.rs`:

```rust
// Add to imports at top of file
use crate::parser::{Parser, state::{Format, Query, QueryResponse}};

// Update AppState struct
#[derive(Clone)]
pub struct AppState {
    pub input_tx: mpsc::Sender<Bytes>,
    pub output_rx: broadcast::Sender<Bytes>,
    pub shutdown: ShutdownCoordinator,
    pub parser: Parser,
}
```

**Step 2: Add screen endpoint**

```rust
#[derive(Deserialize)]
struct ScreenQuery {
    #[serde(default)]
    format: Format,
}

async fn screen(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<ScreenQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let response = state
        .parser
        .query(Query::Screen { format: params.format })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(response))
}
```

**Step 3: Add scrollback endpoint**

```rust
#[derive(Deserialize)]
struct ScrollbackQuery {
    #[serde(default)]
    format: Format,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    100
}

async fn scrollback(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<ScrollbackQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let response = state
        .parser
        .query(Query::Scrollback {
            format: params.format,
            offset: params.offset,
            limit: params.limit,
        })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(response))
}
```

**Step 4: Add Deserialize import and update router**

Add `Deserialize` to serde imports and update router:

```rust
use serde::{Deserialize, Serialize};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/input", post(input))
        .route("/ws/raw", get(ws_raw))
        .route("/screen", get(screen))
        .route("/scrollback", get(scrollback))
        .with_state(state)
}
```

**Step 5: Run cargo check**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles (tests will fail until main.rs is updated)

**Step 6: Commit**

```bash
git add src/api.rs
git commit -m "feat(api): add /screen and /scrollback endpoints"
```

---

## Task 5: Update main.rs to Create Parser

**Files:**
- Modify: `src/main.rs`

**Step 1: Update imports and create parser**

Add parser import:

```rust
use wsh::{api, broker, parser::Parser, pty, shutdown::ShutdownCoordinator, terminal};
```

**Step 2: Update main function to create Parser**

After creating the broker, add:

```rust
let broker = broker::Broker::new();

// Create parser for terminal state tracking
let parser = Parser::spawn(&broker, cols, rows, 10_000);
```

**Step 3: Update AppState creation**

```rust
let state = api::AppState {
    input_tx,
    output_rx: broker.sender(),
    shutdown: shutdown.clone(),
    parser: parser.clone(),
};
```

**Step 4: Add parser resize in signal handler section (if SIGWINCH handling exists)**

Note: SIGWINCH handling will be added in a later task if not present.

**Step 5: Run cargo check**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 6: Run all tests**

Run: `nix develop -c sh -c "cargo test"`
Expected: All tests pass (some may need adjustment)

**Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat: integrate parser into main application"
```

---

## Task 6: Fix API Tests to Include Parser

**Files:**
- Modify: `src/api.rs` (test section)

**Step 1: Update create_test_state helper**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::Broker;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    fn create_test_state() -> (AppState, mpsc::Receiver<Bytes>) {
        let (input_tx, input_rx) = mpsc::channel(64);
        let broker = Broker::new();
        let parser = Parser::spawn(&broker, 80, 24, 1000);
        let state = AppState {
            input_tx,
            output_rx: broker.sender(),
            shutdown: ShutdownCoordinator::new(),
            parser,
        };
        (state, input_rx)
    }

    // ... rest of tests unchanged
}
```

**Step 2: Add tests for new endpoints**

```rust
#[tokio::test]
async fn test_screen_endpoint() {
    let (state, _input_rx) = create_test_state();
    let app = router(state);

    let response = app
        .oneshot(Request::builder().uri("/screen").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("lines").is_some());
    assert!(json.get("cursor").is_some());
}

#[tokio::test]
async fn test_screen_endpoint_plain_format() {
    let (state, _input_rx) = create_test_state();
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/screen?format=plain")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_scrollback_endpoint() {
    let (state, _input_rx) = create_test_state();
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrollback")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("lines").is_some());
    assert!(json.get("total_lines").is_some());
}

#[tokio::test]
async fn test_scrollback_endpoint_with_pagination() {
    let (state, _input_rx) = create_test_state();
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrollback?offset=10&limit=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

**Step 3: Run tests**

Run: `nix develop -c sh -c "cargo test"`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/api.rs
git commit -m "test(api): update tests for parser integration and new endpoints"
```

---

## Task 7: Add /ws/json WebSocket Endpoint

**Files:**
- Modify: `src/api.rs`

**Step 1: Add ws_json handler**

```rust
use crate::parser::events::{EventType, Subscribe};
use tokio_stream::StreamExt;

async fn ws_json(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_json(socket, state))
}

async fn handle_ws_json(socket: WebSocket, state: AppState) {
    let (_guard, mut shutdown_rx) = state.shutdown.register();
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Send connected message
    let connected_msg = serde_json::json!({ "connected": true });
    if ws_tx
        .send(Message::Text(connected_msg.to_string()))
        .await
        .is_err()
    {
        return;
    }

    // Wait for subscribe message
    let subscribe: Subscribe = loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Subscribe>(&text) {
                            Ok(sub) => break sub,
                            Err(e) => {
                                let err = serde_json::json!({
                                    "error": format!("invalid subscribe message: {}", e),
                                    "code": "invalid_subscribe"
                                });
                                let _ = ws_tx.send(Message::Text(err.to_string())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return,
                    _ => continue,
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    return;
                }
            }
        }
    };

    // Confirm subscription
    let subscribed_msg = serde_json::json!({
        "subscribed": subscribe.events.iter().map(|e| format!("{:?}", e).to_lowercase()).collect::<Vec<_>>()
    });
    if ws_tx
        .send(Message::Text(subscribed_msg.to_string()))
        .await
        .is_err()
    {
        return;
    }

    // Subscribe to parser events
    let mut events = state.parser.subscribe();
    let format = subscribe.format;
    let subscribed_types = subscribe.events;

    // Main event loop
    loop {
        tokio::select! {
            event = events.next() => {
                match event {
                    Some(event) => {
                        // Filter based on subscription
                        let should_send = match &event {
                            crate::parser::events::Event::Line { .. } => {
                                subscribed_types.contains(&EventType::Lines)
                            }
                            crate::parser::events::Event::Cursor { .. } => {
                                subscribed_types.contains(&EventType::Cursor)
                            }
                            crate::parser::events::Event::Mode { .. } => {
                                subscribed_types.contains(&EventType::Mode)
                            }
                            crate::parser::events::Event::Diff { .. } => {
                                subscribed_types.contains(&EventType::Diffs)
                            }
                            crate::parser::events::Event::Reset { .. }
                            | crate::parser::events::Event::Sync { .. } => true,
                        };

                        if should_send {
                            if let Ok(json) = serde_json::to_string(&event) {
                                if ws_tx.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    None => break,
                }
            }

            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle resubscribe (simplified: just acknowledge)
                        if let Ok(_sub) = serde_json::from_str::<Subscribe>(&text) {
                            // In a full implementation, we'd update the filter
                            let ack = serde_json::json!({ "subscribed": true });
                            let _ = ws_tx.send(Message::Text(ack.to_string())).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => continue,
                }
            }

            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    let close_frame = CloseFrame {
                        code: axum::extract::ws::close_code::NORMAL,
                        reason: "server shutting down".into(),
                    };
                    let _ = ws_tx.send(Message::Close(Some(close_frame))).await;
                    break;
                }
            }
        }
    }
}
```

**Step 2: Update router to include /ws/json**

```rust
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/input", post(input))
        .route("/ws/raw", get(ws_raw))
        .route("/ws/json", get(ws_json))
        .route("/screen", get(screen))
        .route("/scrollback", get(scrollback))
        .with_state(state)
}
```

**Step 3: Run cargo check**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/api.rs
git commit -m "feat(api): add /ws/json WebSocket endpoint with subscribe protocol"
```

---

## Task 8: Add Integration Tests for New Endpoints

**Files:**
- Create: `tests/parser_integration.rs`

**Step 1: Create integration test file**

```rust
// tests/parser_integration.rs
use bytes::Bytes;
use wsh::broker::Broker;
use wsh::parser::{state::Format, Parser, Query, QueryResponse};

#[tokio::test]
async fn test_parser_with_ansi_sequences() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    // Send colored text
    broker.publish(Bytes::from("\x1b[31mRed Text\x1b[0m Normal"));

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = parser
        .query(Query::Screen {
            format: Format::Styled,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            // Should have parsed the content
            assert!(!screen.lines.is_empty());
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_cursor_movement() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    // Move cursor to row 5, col 10
    broker.publish(Bytes::from("\x1b[5;10H"));

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = parser.query(Query::Cursor).await.unwrap();

    match response {
        QueryResponse::Cursor(cursor) => {
            // Cursor should have moved (0-indexed)
            assert_eq!(cursor.cursor.row, 4);
            assert_eq!(cursor.cursor.col, 9);
        }
        _ => panic!("expected Cursor response"),
    }
}

#[tokio::test]
async fn test_parser_plain_vs_styled() {
    let broker = Broker::new();
    let parser = Parser::spawn(&broker, 80, 24, 1000);

    broker.publish(Bytes::from("\x1b[1mBold\x1b[0m"));

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Get plain format
    let plain = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    // Get styled format
    let styled = parser
        .query(Query::Screen {
            format: Format::Styled,
        })
        .await
        .unwrap();

    match (plain, styled) {
        (QueryResponse::Screen(p), QueryResponse::Screen(s)) => {
            // Both should have the text
            assert!(!p.lines.is_empty());
            assert!(!s.lines.is_empty());
        }
        _ => panic!("expected Screen responses"),
    }
}
```

**Step 2: Run integration tests**

Run: `nix develop -c sh -c "cargo test parser_integration"`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/parser_integration.rs
git commit -m "test: add parser integration tests"
```

---

## Task 9: Update Implementation Roadmap

**Files:**
- Modify: `docs/plans/2026-02-03-implementation-roadmap.md`

**Step 1: Update Phase 2 status**

Update the status table and Phase 2 section to mark items as complete.

**Step 2: Commit**

```bash
git add docs/plans/2026-02-03-implementation-roadmap.md
git commit -m "docs: update roadmap to reflect Phase 2 progress"
```

---

## Task 10: Final Testing and Cleanup

**Step 1: Run full test suite**

Run: `nix develop -c sh -c "cargo test"`
Expected: All tests pass

**Step 2: Run clippy**

Run: `nix develop -c sh -c "cargo clippy -- -D warnings"`
Expected: No warnings

**Step 3: Run manual smoke test**

Run: `nix develop -c sh -c "cargo run"`

In another terminal:
```bash
# Test /screen
curl http://localhost:8080/screen | jq .

# Test /screen?format=plain
curl 'http://localhost:8080/screen?format=plain' | jq .

# Test /scrollback
curl http://localhost:8080/scrollback | jq .

# Test /ws/json (requires websocat)
echo '{"events": ["lines", "cursor"]}' | websocat ws://localhost:8080/ws/json
```

**Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: address issues found during final testing"
```

---

## Summary

This plan implements Phase 2 in 10 tasks:

1. **Parser module skeleton** - Basic structure and types
2. **Parser task implementation** - Event emission and query handling
3. **Parser unit tests** - Verify parser behavior
4. **HTTP endpoints** - /screen and /scrollback
5. **Main integration** - Wire parser into application
6. **API test updates** - Fix existing tests
7. **WebSocket endpoint** - /ws/json with subscribe protocol
8. **Integration tests** - End-to-end parser tests
9. **Documentation** - Update roadmap
10. **Final testing** - Smoke tests and cleanup

Each task is atomic and can be committed independently. The plan follows TDD where practical and maintains backward compatibility with existing endpoints.
