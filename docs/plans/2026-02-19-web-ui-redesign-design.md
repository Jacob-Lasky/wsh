# Web UI Redesign: Sidebar, View Modes, Quiescence Queue

## Overview

Redesign the web frontend around a sidebar + main content layout with tag-based session grouping, three view modes (carousel, tiled, quiescence queue), drag-and-drop tag management, a command palette, and polished theming. The goal is a UI that feels intimate with 1-2 sessions and scales gracefully to dozens -- serving the human operator, agent supervisor, and demo showcase personas equally.

**Approach:** Hybrid. Keep `Terminal.tsx`, `WshClient`, `api/types.ts`, and Preact Signals infrastructure. Rebuild the layout shell, sidebar, view modes, and navigation from scratch.

**Backend changes:** None. The existing API surface (sessions, tags, quiescence, subscriptions, lifecycle events) covers all requirements.

## Layout Shell

Two-panel layout: collapsible sidebar (left) + main content area (right). No top nav bar.

### Sidebar

Default width ~15% of viewport. Resizable via drag handle on right edge. Collapsible to a narrow icon bar (~40px) showing only group icons and badge counts. Scrollable if groups exceed viewport height.

**Structure (top to bottom):**

1. **"All Sessions" group** -- always first. Shows total session count. Cannot receive drag-drops of individual sessions. Dragging this group onto another group tags all sessions (respecting modifier key behavior).
2. **Tag-based groups** -- one per unique tag, alphabetical. Each shows:
   - Tag name (click to edit inline)
   - Badge count for sessions needing attention (quiescent + exited)
   - Live tiled mini-preview of the group's sessions (tiny real terminal renders)
   - Status dots per mini-session: green = running, amber = quiescent, grey = exited
   - "Last active Xm ago" timestamp beneath the preview
3. **"Untagged" group** -- always last. Collects sessions with no tags.
4. **Bottom section** -- connection status dot, theme picker button, search icon (opens command palette), keyboard shortcut help icon (?), new session button.

**Multi-select:** Ctrl+click or Shift+click to select multiple groups. Main area shows the union of their sessions.

### Tag Editing

Click a tag name on any group or session: inline popover with editable text field, autocomplete from existing tags, "x" to remove. "+" button on a session to add a tag. Changes take effect immediately; groups re-cluster with smooth animation.

## View Modes

Three modes, switchable via a small icon toggle in the top-right of the main area. The active mode is remembered per-group (persisted to localStorage).

### Carousel Mode

**Desktop:** 3D depth effect. Center session ~70% of main area width, fully interactive (receives keystrokes). Adjacent sessions peek from left/right at ~60% scale with perspective transform and reduced opacity. Smooth animated transitions on rotate.

**Mobile:** Full-width single session. No side previews. Swipe left/right to rotate. Subtle edge shadows hint at more sessions.

**Navigation:** Super+Left/Right on desktop. Click side preview to snap to center. Swipe on mobile.

The center session has a visible focus indicator (subtle glow or ring).

### Tiled Mode

**Auto-grid layout.** Algorithm picks the most balanced NxM arrangement. Intelligent space filling: no empty cells. 3 sessions = 2 top + 1 full-width bottom. 5 sessions = 3 top + 2 wider bottom. Always minimizes wasted space.

**Interaction:**
- Drag a tile onto another to swap positions. Drop target highlights; dragged tile becomes semi-transparent.
- Resize handles at grid borders for relative sizing.
- Click a tile to focus it (receives keyboard input, gets focus indicator). Stays in tiled view.
- Double-click to maximize (switches to carousel mode focused on that session).

### Queue Mode (Quiescence Queue)

Scoped to the selected sidebar group. FIFO by quiescence time.

**Layout:**
- **Top bar, left:** Pending queue -- small thumbnails of sessions that reached quiescence, in FIFO order. First item is the one displayed full-size below.
- **Top bar, right:** Handled/active sessions -- thumbnails of sessions that are dismissed or haven't reached quiescence. Muted styling.
- **Center:** Current queued session, full-size, fully interactive.
- **Dismiss:** Prominent checkmark/Done button + Super+Enter keyboard shortcut. Moves session to "handled", re-subscribes to quiescence, next queued session slides in.
- **Empty state:** "All caught up" message with handled/active sessions still visible in top bar.

## Drag & Drop

Every drag operation maps to tag manipulation.

**Sources:** Sessions (from any view) and groups (from sidebar).

**Targets:** Sidebar groups. Within tiled mode, dropping on another session swaps positions (layout operation, not tag operation).

**Modifier behavior:**
- **Default (no modifier):** Replace. Remove all existing tags, add target group's tag. Session moves between groups.
- **Shift+drop:** Add. Keep existing tags, add target group's tag. Session appears in multiple groups.

**Visual feedback:**
- Dragged item: semi-transparent, slight scale-up.
- Valid drop targets: border glow / background tint.
- Label near cursor: "Move to [tag]" by default, "Also add to [tag]" when Shift held. Updates live as Shift state changes.
- Invalid targets: no-drop cursor.

**Edge case:** Dragging the last session out of a tag group dissolves the group (smooth collapse animation).

## Theming

### Theme Picker

Click theme button in sidebar bottom section: context menu/dropdown with color swatch previews (4-5 dots of key colors) + theme name. Selection applies with ~300ms crossfade.

### Themes

1. **Glass** -- Frosted translucency, backdrop blur, soft shadows, rounded corners. macOS feel.
2. **Neon** -- Cyberpunk glow. Subtle scanlines. Selective glow on active/focused elements.
3. **Minimal** -- Clean, quiet. Thin borders, muted palette, no blur.
4. **Tokyo Night** -- Dark blue-purple (#1a1b26 bg, #a9b1d6 fg, #7aa2f7 accent).
5. **Catppuccin Mocha** -- Pastel-on-dark, soft, cozy.
6. **Dracula** -- Purple/pink/green on dark. The classic.

### Polish Details

- Consistent border-radius across all elements.
- Entrance animations: sidebar groups fade-slide in, sessions transition smoothly on group switch.
- Focus states: soft glow or animated border, never browser default outline.
- Scrollbars styled to match theme (thin, colored, fade on idle).
- Cursor blink animation matches theme energy (steady for minimal, glow pulse for neon).
- Hover states on every interactive element.
- Transitions on everything that changes: tag re-clustering, group reordering, view mode switching, sidebar collapse/expand. No jarring pops.

### Typography

- Monospace for terminal content (JetBrains Mono / Fira Code / user configured).
- Clean sans-serif for UI chrome (Inter / system-ui). Small, understated.

## Keyboard Shortcuts

**Primary modifier: Super (Meta/Cmd).** Fallback: Ctrl+Shift for environments where Super is unavailable.

| Shortcut | Action |
|----------|--------|
| Super+Left/Right | Carousel rotate |
| Super+Enter | Dismiss session in queue mode |
| Super+1-9 | Jump to Nth session in group |
| Super+Tab / Shift+Tab | Cycle sidebar groups |
| Super+B | Toggle sidebar collapse |
| Super+T | Open theme picker |
| Super+N | New session |
| Super+W | Kill focused session (with confirmation) |
| Super+F | Carousel mode |
| Super+G | Tiled mode |
| Super+Q | Queue mode |
| Super+Arrow keys | Move focus between tiles (tiled mode) |
| Super+K | Command palette |
| Super+? | Keyboard shortcut cheat sheet |

### Command Palette (Super+K)

Fuzzy search across sessions (by name, matched first), groups/tags, and actions (new session, kill session, switch theme, toggle sidebar, etc.). Ranked results with category labels. Arrow keys to navigate, Enter to select, Escape to dismiss. Also accessible via search icon in sidebar.

### Shortcut Cheat Sheet (Super+?)

Modal overlay organized by category (navigation, view modes, session management, tiled mode). Searchable: type to filter. Also accessible via "?" icon in sidebar. Dismisses on Escape or click outside.

## Mobile Adaptation

### Breakpoints

- **< 640px:** Full mobile layout.
- **640-1024px:** Tablet hybrid (overlay sidebar, 2-column tiles).
- **> 1024px:** Full desktop layout.

### Mobile Layout

- Sidebar becomes a **bottom sheet**. Swipe up from tab bar to reveal group list. Tab bar shows: current group name, badge count, "+" for new session.
- **Carousel:** Full-width, swipe to rotate. Subtle edge shadows.
- **Tiled:** Vertical stack (sessions full-width, scrollable). 2x1 on larger phones/tablets.
- **Queue:** Compact strip top bar with pending count badge (not thumbnails). Full-screen current session. Dismiss via swipe-up gesture or floating action button.
- **Drag & drop:** Long-press to initiate. Bottom sheet auto-expands as drop target. "Move / Add" toggle pill replaces Shift modifier.
- **Tag editing:** Popover rendered as bottom sheet modal.

### Accessibility

- All interactive elements keyboard-reachable via Tab (outside terminal focus).
- ARIA labels on sidebar groups, session status badges, view mode toggles.
- High-contrast theme option (WCAG AA).
- `prefers-reduced-motion` respected: depth transitions and fade animations become instant swaps.
- `aria-live` regions for session status changes.

## State Management

### New Signals

```
groups: Signal<Group[]>                       -- derived from sessions + tags
selectedGroups: Signal<string[]>              -- currently selected sidebar groups
viewModePerGroup: Signal<Map<string, ViewMode>>  -- remembered per group
sidebarWidth: Signal<number>                  -- persisted to localStorage
sidebarCollapsed: Signal<boolean>             -- persisted to localStorage
quiescenceQueue: Signal<Map<string, QueueEntry[]>>  -- keyed by tag, FIFO
```

### Preserved from Current Implementation

- Per-session `ScreenState` signals
- `WshClient` WebSocket class (JSON-RPC, reconnection, Happy Eyeballs)
- Session lifecycle event handling
- Auth token flow

### Persistence (localStorage)

Sidebar width, collapsed state, theme, zoom level, auth token, view mode per group, tile layout positions per group.

## Components to Keep

- `Terminal.tsx` -- core terminal renderer (lines, cursor, styled spans, scrollback)
- `InputBar.tsx` -- input field with key/sequence mapping (may need minor adaptation)
- `ErrorBoundary.tsx` -- error boundary
- `src/api/ws.ts` -- WshClient WebSocket class
- `src/api/types.ts` -- TypeScript types
- `src/state/terminal.ts` -- per-session screen state signals

## Components to Rebuild

- Layout shell (sidebar + main area replacing the current single-panel layout)
- Sidebar (groups, mini-previews, tag editing, bottom section)
- Carousel (3D depth effect replacing flat scroll-snap carousel)
- Tiled view (auto-grid with intelligent filling replacing horizontal-only splits)
- Queue view (entirely new)
- Status indicators (replacing the current bottom status bar)
- Theme picker (context menu replacing cycle button)
- Command palette (new)
- Shortcut cheat sheet (new)
- Drag-and-drop system (new)
- Mobile bottom sheet (replacing current swipe-up gesture)

## Components to Remove

- `SessionCarousel.tsx` -- replaced by new depth carousel
- `SessionGrid.tsx` -- replaced by sidebar group navigation
- `SessionThumbnail.tsx` -- replaced by sidebar mini-previews
- `TiledLayout.tsx` -- replaced by new auto-grid tiled view
- `StatusBar.tsx` -- functionality distributed to sidebar bottom + main area header
- `PageIndicator.tsx` -- replaced by sidebar navigation
