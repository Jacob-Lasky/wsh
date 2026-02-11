# Panels Implementation Plan

Design: [2026-02-10-panels-design.md](2026-02-10-panels-design.md)

## Step 1: Panel Data Model (`src/panel/types.rs`)

Create the `Panel` type and supporting enums. Reuse `OverlaySpan` from the
overlay module for styled content.

```rust
// src/panel/types.rs

pub type PanelId = String;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Bottom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Panel {
    pub id: PanelId,
    pub position: Position,
    pub height: u16,
    pub z: i32,
    pub spans: Vec<OverlaySpan>,   // reuse from overlay::types
    pub visible: bool,              // read-only, computed by layout
}
```

- `visible` is always serialized (agents need to know)
- `spans` reuses `OverlaySpan` directly -- no new span type
- `Position` uses lowercase serde for clean JSON (`"top"`, `"bottom"`)

**Tests:** Serde round-trip for Panel, Position enum serialization.

---

## Step 2: Panel Store (`src/panel/store.rs`)

Thread-safe CRUD store mirroring `OverlayStore`. Same
`Arc<RwLock<HashMap>>` pattern.

**Methods:**

- `create(position, height, z: Option<i32>, spans) -> PanelId`
  Auto-increment z if None. Returns new UUID.
- `get(id) -> Option<Panel>`
- `list() -> Vec<Panel>` -- sorted by position (Top first), then z
  descending (highest z first, i.e., closest to edge first).
- `update(id, spans) -> bool` -- replace spans only
- `patch(id, position?, height?, z?) -> bool` -- partial update
- `delete(id) -> bool`
- `clear()`

The store does NOT compute visibility. That's the layout engine's job.
Panels returned from the store always have `visible: true` as a default;
the layout engine overwrites this.

**Tests:** CRUD operations, list sorting, auto-increment z.

---

## Step 3: Layout Engine (`src/panel/layout.rs`)

Pure function that computes the screen layout given all panels and the
terminal size.

```rust
pub struct Layout {
    pub top_panels: Vec<Panel>,     // visible top panels, ordered edge->content
    pub bottom_panels: Vec<Panel>,  // visible bottom panels, ordered edge->content
    pub hidden_panels: Vec<PanelId>,
    pub scroll_region_top: u16,     // 1-indexed, first PTY row
    pub scroll_region_bottom: u16,  // 1-indexed, last PTY row
    pub pty_rows: u16,              // rows available for PTY
    pub pty_cols: u16,              // unchanged from terminal cols
}

pub fn compute_layout(
    panels: &[Panel],
    terminal_rows: u16,
    terminal_cols: u16,
) -> Layout
```

**Algorithm:**

1. Separate panels by position (top vs bottom).
2. Sort each group by z descending (highest z = highest priority).
3. Greedily allocate rows, starting from highest z.
4. Stop when remaining rows <= 1 (minimum 1 PTY row).
5. Remaining panels are hidden.
6. Compute scroll region: top = sum of visible top panel heights + 1,
   bottom = terminal_rows - sum of visible bottom panel heights.
7. Set `visible = true` on allocated panels, `visible = false` on hidden.

**Tests:** This is the most critical module. Test cases:
- No panels -> full terminal is PTY
- Single top panel -> correct scroll region
- Single bottom panel -> correct scroll region
- Both top and bottom panels
- Panels exceeding terminal height -> lowest z hidden
- Exactly 1 row remaining for PTY
- Terminal with 1 row, no panels possible
- Multiple panels same position, different z
- Z-index ordering determines which panels are hidden

---

## Step 4: Panel Rendering (`src/panel/render.rs`)

ANSI escape sequence rendering for panels. Similar to overlay rendering but
positions content in specific row ranges outside the scroll region.

**Functions:**

- `render_panel(panel, start_row) -> String`
  Renders a single panel starting at `start_row` (0-indexed terminal row).
  For each row in the panel's height: position cursor, render spans for
  that row (split on `\n`), clear remaining columns with spaces.

- `render_all_panels(layout, terminal_cols) -> String`
  Renders all visible panels. Wraps in save/restore cursor.
  Top panels: render from row 0 downward.
  Bottom panels: render from scroll_region_bottom + 1 downward.

- `erase_all_panels(layout, terminal_cols) -> String`
  Overwrites all panel rows with spaces. Used before re-rendering.

- `set_scroll_region(top, bottom) -> String`
  Returns `\x1b[{top};{bottom}r`

- `reset_scroll_region() -> String`
  Returns `\x1b[r`

Reuse `begin_sync`, `end_sync`, `save_cursor`, `restore_cursor`,
`cursor_position`, `render_span_style`, `reset` from `overlay::render`.
These should be factored into a shared location or the panel module should
call them directly from `overlay::render` (they're already `pub`).

**Tests:** Render output for single panel, multi-row panel, panel with
styled spans, erase output, scroll region escape sequences.

---

## Step 5: Panel Module Entry (`src/panel/mod.rs`, `src/lib.rs`)

- Create `src/panel/mod.rs` re-exporting public types and functions.
- Add `pub mod panel;` to `src/lib.rs`.

---

## Step 6: Layout Coordinator in Main

This is the most complex integration step. `main.rs` needs to:

1. Create a `PanelStore` alongside the `OverlayStore`.
2. Track terminal dimensions (rows, cols) in shared state.
3. Provide a `reconfigure_layout()` function that panel API handlers can
   call after mutations.

**Shared terminal size:**

```rust
// In main.rs or a new module
pub struct TerminalSize {
    inner: Arc<RwLock<(u16, u16)>>,  // (rows, cols)
}
```

Needed because layout computation requires current terminal dimensions, and
API handlers need access to it.

**`reconfigure_layout()` function:**

Called by API handlers after any panel mutation that could change total
height. Also called on outer terminal resize.

```
fn reconfigure_layout(panels, terminal_size, pty, parser) {
    1. let all_panels = panels.list()
    2. let (rows, cols) = terminal_size.get()
    3. let layout = compute_layout(&all_panels, rows, cols)
    4. Update panel visibility in store based on layout
    5. Write to stdout:
       a. begin_sync
       b. erase old panels
       c. set_scroll_region(layout.scroll_region_top, layout.scroll_region_bottom)
       d. render all visible panels
       e. end_sync
    6. Resize PTY: pty.resize(layout.pty_rows, layout.pty_cols)
    7. Resize parser: parser.resize(layout.pty_cols, layout.pty_rows)
}
```

This function needs access to: `PanelStore`, terminal size, PTY handle,
parser handle, and stdout. We'll pass these through `AppState` or a
dedicated `LayoutContext`.

**PTY handle sharing:**

Currently `Pty` is consumed in `main()` -- reader/writer/child are taken
out. The `Pty` struct itself (with `resize()`) needs to be shared with API
handlers. Wrap it in `Arc` and add to `AppState`.

**SIGWINCH handling:**

Add a `tokio::signal::unix::signal(SignalKind::window_change())` handler
in `main()`. On SIGWINCH:
1. Query new terminal size via `crossterm::terminal::size()`
2. Update shared terminal size
3. Call `reconfigure_layout()`

If no panels exist, just resize PTY + parser directly (no scroll region
management needed).

**PTY reader changes:**

The PTY reader currently does the overlay erase/render cycle. With panels,
the PTY output is confined to the scroll region by DECSTBM, so panels are
NOT affected by PTY output. No changes needed to the PTY reader for panel
rendering -- panels are stable outside the scroll region.

However, the PTY reader needs the `PanelStore` to know whether to also
erase/re-render overlays that might be in the panel region. For now, this
is the agent's problem (per design doc), so no change needed.

---

## Step 7: AppState & API Wiring

**Extend `AppState`:**

```rust
pub struct AppState {
    // ... existing fields ...
    pub panels: PanelStore,
    pub pty: Arc<Pty>,              // for resize
    pub terminal_size: TerminalSize, // for layout computation
}
```

**Add HTTP routes** in `src/api/mod.rs`:

```rust
.route("/panel", get(panel_list).post(panel_create).delete(panel_clear))
.route("/panel/:id", get(panel_get).put(panel_update).patch(panel_patch).delete(panel_delete))
```

**Add HTTP handlers** in `src/api/handlers.rs`:

Mirror the overlay handlers. Key difference: mutations that change height
or position call `reconfigure_layout()` instead of just flushing.

- `panel_create` -> create in store, reconfigure_layout, return 201 + id
- `panel_list` -> return all panels with visibility computed
- `panel_get` -> return single panel with visibility
- `panel_update` -> full replace (spans + height + position + z),
  reconfigure if height/position/z changed, else just re-render
- `panel_patch` -> partial update, reconfigure if height/position/z
  changed, else just re-render
- `panel_delete` -> delete, reconfigure_layout, return 204
- `panel_clear` -> clear all, reconfigure_layout (reset scroll region),
  return 204

**Span-only updates:** If only spans changed (no height/position/z change),
skip the PTY resize and just re-render the affected panel content in its
existing rows.

`flush_panel_to_stdout(panel, start_row, terminal_cols)` -- render a single
panel's content without changing scroll region or PTY size.

**Add `PanelNotFound` error variant** to `src/api/error.rs`.

---

## Step 8: WebSocket Panel Methods (`src/api/ws_methods.rs`)

Add panel param types:

```rust
struct CreatePanelParams { position, height, z: Option<i32>, spans: Option<Vec<OverlaySpan>> }
struct PanelIdParams { id }
struct UpdatePanelParams { id, position, height, z, spans }
struct PatchPanelParams { id, position?, height?, z?, spans? }
```

Add to `dispatch()`:

- `"create_panel"` -> parse CreatePanelParams, create, reconfigure, return
- `"list_panels"` -> return panels with visibility
- `"get_panel"` -> return panel with visibility
- `"update_panel"` -> full replace, reconfigure if needed
- `"patch_panel"` -> partial update, reconfigure if needed
- `"delete_panel"` -> delete, reconfigure
- `"clear_panels"` -> clear all, reconfigure

Same pattern as overlay methods.

**Tests:** Unit tests for all panel dispatch methods, matching the overlay
test patterns.

---

## Step 9: Documentation Updates

### `docs/api/panels.md` (new)

Mirror `docs/api/overlays.md` structure:
- Concepts: position, z-index stacking, visibility, PTY resizing
- Create: POST /panel
- List: GET /panel
- Get: GET /panel/:id
- Update: PUT /panel/:id
- Patch: PATCH /panel/:id
- Delete: DELETE /panel/:id
- Clear: DELETE /panel
- Spans: link to overlay span documentation
- Example: agent status bar use case

### `docs/api/README.md`

- Add panel endpoints to the "Endpoints at a Glance" table
- Add a "Panels" section with link to panels.md
- Update WebSocket section to mention panel methods

### `docs/api/websocket.md`

- Add Panel Methods section mirroring Overlay Methods
- Document all 7 panel methods with params and examples

### `docs/api/openapi.yaml`

Add schemas:
- `Position` (enum: top, bottom)
- `Panel` (id, position, height, z, spans, visible)
- `CreatePanelRequest` (position required, height required, z optional,
  spans optional)
- `UpdatePanelRequest` (position, height, z, spans -- all required)
- `PatchPanelRequest` (all optional: position, height, z, spans)

Add paths:
- `/panel` (GET, POST, DELETE)
- `/panel/{id}` (GET, PUT, PATCH, DELETE)

All with request/response schemas and error responses.

---

## Step 10: Integration Tests

End-to-end tests verifying:

- Create panel -> PTY resize event emitted (via parser Reset event)
- Delete panel -> PTY resize back to original
- Multiple panels -> correct cumulative PTY size reduction
- Panel visibility -> hidden panels don't affect PTY size
- Span-only update -> no PTY resize
- HTTP and WebSocket round-trips for all CRUD operations

---

## Dependency Graph

```
Step 1 (types) ─────────────┐
                             ├──► Step 5 (mod.rs, lib.rs)
Step 2 (store) ─────────────┤
                             │
Step 3 (layout) ────────────┤
                             │
Step 4 (render) ────────────┘
                                   │
                                   ▼
                            Step 6 (main.rs integration)
                                   │
                                   ▼
                            Step 7 (AppState + HTTP handlers)
                                   │
                                   ▼
                            Step 8 (WebSocket methods)
                                   │
                                   ▼
                            Step 9 (documentation)
                                   │
                                   ▼
                            Step 10 (integration tests)
```

Steps 1-4 can be developed in parallel (they only depend on each other at
the interface level). Steps 5-8 are sequential. Step 9 can start as soon
as the API is stable. Step 10 comes last.
