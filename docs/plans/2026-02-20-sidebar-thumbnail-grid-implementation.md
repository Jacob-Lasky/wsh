# Sidebar Thumbnail Grid Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the dual mini-preview + text session list sidebar with a single thumbnail grid per group, with hover-based name/status/tag interactions.

**Architecture:** The sidebar group body changes from `<MiniViewPreview>` + `<session-list>` + `<timestamp>` to a single CSS grid of `<ThumbnailCell>` components. Status colors are corrected (green=idle, amber=running). Groups become collapsible with colored count chips. "All Sessions" moves to last. Queue view labels change from "Pending/Active" to "Idle/Running". Dead "exited" state code is removed throughout.

**Tech Stack:** Preact, Preact Signals, CSS (no new deps)

---

### Task 1: Fix status color mapping (green=idle, amber=running)

The current mapping is backwards. Green means "running" and amber means "quiescent". It should be green=idle/quiescent, amber=running.

**Files:**
- Modify: `web/src/components/Sidebar.tsx:22-27` (StatusDot component)
- Modify: `web/src/styles/terminal.css:1359-1369` (dot colors — no change needed, just the class assignments)

**Step 1: Update StatusDot in Sidebar.tsx**

In `web/src/components/Sidebar.tsx`, change the `StatusDot` component (lines 22-27):

```tsx
function StatusDot({ status }: { status: SessionStatus | undefined }) {
  const cls = status === "quiescent" ? "status-dot-green"
    : "status-dot-amber";
  return <span class={`mini-status-dot ${cls}`} aria-label={statusLabel(status)} />;
}
```

Also update `statusLabel` (lines 16-20):

```tsx
function statusLabel(status: SessionStatus | undefined): string {
  return status === "quiescent" ? "Idle" : "Running";
}
```

**Step 2: Verify the CSS color values are sensible**

Check `web/src/styles/terminal.css:1359-1369`. Green (`#4ade80`) for idle and amber (`#fbbf24`) for running are correct semantic colors. No CSS changes needed.

**Step 3: Commit**

```bash
git add web/src/components/Sidebar.tsx
git commit -m "fix(web): correct status dot colors — green=idle, amber=running"
```

---

### Task 2: Remove dead "exited" state code

The server removes sessions on process exit (`monitor_child_exit` in `src/session.rs`). The "exited" UI state is dead code.

**Files:**
- Modify: `web/src/state/groups.ts:25` (SessionStatus type)
- Modify: `web/src/state/groups.ts:54-56,68-70,81-83` (badge count filters)
- Modify: `web/src/app.tsx:359-371` (session_exited handler)
- Modify: `web/src/styles/terminal.css:1367-1369` (`.status-dot-grey`)

**Step 1: Remove "exited" from SessionStatus type**

In `web/src/state/groups.ts:25`, change:

```ts
export type SessionStatus = "running" | "quiescent";
```

**Step 2: Remove "exited" from badge count filters**

In `web/src/state/groups.ts`, the badge counts filter for `"quiescent" || "exited"`. Remove the `"exited"` checks at lines 55, 69, and 82. Each should become just:

```ts
(s) => statuses.get(s) === "quiescent"
```

**Step 3: Remove session_exited handler from app.tsx**

In `web/src/app.tsx:359-371`, delete the entire `case "session_exited"` block.

**Step 4: Remove .status-dot-grey CSS**

In `web/src/styles/terminal.css:1367-1369`, delete:

```css
.status-dot-grey {
  background: #6b7280;
}
```

**Step 5: Commit**

```bash
git add web/src/state/groups.ts web/src/app.tsx web/src/styles/terminal.css
git commit -m "fix(web): remove dead 'exited' session state — sessions are destroyed on exit"
```

---

### Task 3: Reorder groups — "All Sessions" last

Currently `groups` computed signal puts "All Sessions" first. Move it to last.

**Files:**
- Modify: `web/src/state/groups.ts:51-63` (groups computed)

**Step 1: Move "All Sessions" to end of groups array**

In `web/src/state/groups.ts`, the `groups` computed signal builds `result` array. Currently "All Sessions" is pushed first (lines 57-63), then custom tags, then "Untagged". Change the order to: custom tags first, then "Untagged", then "All Sessions" last.

Replace lines 51-93:

```ts
  const result: Group[] = [];

  // Custom tag groups (sorted alphabetically)
  const sortedTags = Array.from(tagGroups.keys()).sort();
  for (const tag of sortedTags) {
    const sessions = tagGroups.get(tag)!;
    const badge = sessions.filter(
      (s) => statuses.get(s) === "quiescent"
    ).length;
    result.push({
      tag,
      label: tag,
      sessions,
      isSpecial: false,
      badgeCount: badge,
    });
  }

  // Untagged (only if sessions exist)
  if (untagged.length > 0) {
    const badge = untagged.filter(
      (s) => statuses.get(s) === "quiescent"
    ).length;
    result.push({
      tag: "untagged",
      label: "Untagged",
      sessions: untagged,
      isSpecial: true,
      badgeCount: badge,
    });
  }

  // All Sessions (last — grows fastest, least useful for navigation)
  const allSessions = Array.from(infoMap.keys());
  const allBadge = allSessions.filter(
    (s) => statuses.get(s) === "quiescent"
  ).length;
  result.push({
    tag: "all",
    label: "All Sessions",
    sessions: allSessions,
    isSpecial: true,
    badgeCount: allBadge,
  });

  return result;
```

**Step 2: Update default selectedGroups**

In `web/src/state/groups.ts:18`, change the default selected group. Since "All Sessions" is now last rather than first, keep `["all"]` as the default — it still works fine as the initial selection.

No change needed — `selectedGroups` uses tag strings, not indices.

**Step 3: Commit**

```bash
git add web/src/state/groups.ts
git commit -m "feat(web): reorder sidebar groups — 'All Sessions' last"
```

---

### Task 4: Add collapsible group state

Add a `collapsedGroups` signal persisted to localStorage.

**Files:**
- Modify: `web/src/state/groups.ts` (add signal + helpers)

**Step 1: Add collapsedGroups signal**

In `web/src/state/groups.ts`, after the `selectedGroups` signal (line 18), add:

```ts
const storedCollapsed: string[] = JSON.parse(localStorage.getItem("wsh-collapsed-groups") || "[]");
export const collapsedGroups = signal<Set<string>>(new Set(storedCollapsed));

export function toggleGroupCollapsed(tag: string): void {
  const updated = new Set(collapsedGroups.value);
  if (updated.has(tag)) {
    updated.delete(tag);
  } else {
    updated.add(tag);
  }
  collapsedGroups.value = updated;
  localStorage.setItem("wsh-collapsed-groups", JSON.stringify(Array.from(updated)));
}
```

**Step 2: Commit**

```bash
git add web/src/state/groups.ts
git commit -m "feat(web): add collapsible group state with localStorage persistence"
```

---

### Task 5: Add Group helper — running/idle counts

The collapsed group header needs running and idle session counts for the colored chips. Add a helper that computes these from the group's sessions and the status map.

**Files:**
- Modify: `web/src/state/groups.ts` (add to Group interface or as helper)

**Step 1: Add count helper**

In `web/src/state/groups.ts`, add a helper function at the bottom:

```ts
export function getGroupStatusCounts(group: Group): { running: number; idle: number } {
  const statuses = sessionStatuses.value;
  let idle = 0;
  let running = 0;
  for (const s of group.sessions) {
    if (statuses.get(s) === "quiescent") {
      idle++;
    } else {
      running++;
    }
  }
  return { running, idle };
}
```

**Step 2: Commit**

```bash
git add web/src/state/groups.ts
git commit -m "feat(web): add getGroupStatusCounts helper for collapsed group chips"
```

---

### Task 6: Build ThumbnailCell component

This is the new core component — a single session thumbnail with hover overlay.

**Files:**
- Create: `web/src/components/ThumbnailCell.tsx`

**Step 1: Create the ThumbnailCell component**

Create `web/src/components/ThumbnailCell.tsx`:

```tsx
import { useState, useRef, useEffect, useCallback } from "preact/hooks";
import type { WshClient } from "../api/ws";
import { sessionStatuses, type SessionStatus } from "../state/groups";
import { focusedSession } from "../state/sessions";
import { MiniTermContent } from "./MiniViewPreview";
import { TagEditor } from "./TagEditor";

interface ThumbnailCellProps {
  session: string;
  client: WshClient;
}

function statusLabel(status: SessionStatus | undefined): string {
  return status === "quiescent" ? "Idle" : "Running";
}

export function ThumbnailCell({ session, client }: ThumbnailCellProps) {
  const status = sessionStatuses.value.get(session);
  const dotClass = status === "quiescent" ? "status-dot-green" : "status-dot-amber";
  const [hovered, setHovered] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(session);
  const [showTagEditor, setShowTagEditor] = useState(false);
  const renameRef = useRef<HTMLInputElement>(null);

  // Focus rename input when entering rename mode
  useEffect(() => {
    if (renaming) {
      renameRef.current?.focus();
      renameRef.current?.select();
    }
  }, [renaming]);

  const handleRenameSubmit = useCallback(() => {
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== session) {
      client.updateSession(session, { name: trimmed }).catch((e) => {
        console.error("Failed to rename session:", e);
      });
    }
    setRenaming(false);
  }, [renameValue, session, client]);

  const handleRenameKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleRenameSubmit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      setRenaming(false);
      setRenameValue(session);
    }
  }, [handleRenameSubmit, session]);

  const handleThumbClick = useCallback((e: MouseEvent) => {
    // Don't navigate if clicking on name, tag icon, or rename input
    const target = e.target as HTMLElement;
    if (target.closest(".thumb-name, .thumb-tag-btn, .thumb-rename-input, .tag-editor")) return;
    focusedSession.value = session;
  }, [session]);

  return (
    <div
      class={`thumb-cell ${focusedSession.value === session ? "focused" : ""}`}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => { setHovered(false); if (!showTagEditor) setRenaming(false); }}
      onClick={handleThumbClick}
      role="button"
      aria-label={`Session ${session}, ${statusLabel(status)}`}
    >
      {/* Terminal preview */}
      <div class="thumb-preview">
        <MiniTermContent session={session} />
      </div>

      {/* Status dot — always visible in lower-right */}
      {!hovered && (
        <span class={`thumb-status-dot ${dotClass}`} aria-label={statusLabel(status)} />
      )}

      {/* Hover overlay — bottom bar with name + status dot */}
      {hovered && (
        <div class="thumb-overlay">
          {renaming ? (
            <input
              ref={renameRef}
              type="text"
              class="thumb-rename-input"
              value={renameValue}
              onInput={(e) => setRenameValue((e.target as HTMLInputElement).value)}
              onKeyDown={handleRenameKeyDown}
              onBlur={handleRenameSubmit}
              onClick={(e: MouseEvent) => e.stopPropagation()}
            />
          ) : (
            <span
              class="thumb-name"
              onClick={(e: MouseEvent) => { e.stopPropagation(); setRenaming(true); setRenameValue(session); }}
              title="Click to rename"
            >
              {session}
            </span>
          )}
          <span class={`mini-status-dot ${dotClass}`} />
        </div>
      )}

      {/* Tag icon — upper-right, visible on hover */}
      {hovered && (
        <button
          class="thumb-tag-btn"
          onClick={(e: MouseEvent) => { e.stopPropagation(); setShowTagEditor(!showTagEditor); }}
          title="Edit tags"
        >
          &#9868;
        </button>
      )}

      {/* Tag editor popover */}
      {showTagEditor && (
        <div class="thumb-tag-popover">
          <TagEditor
            session={session}
            client={client}
            onClose={() => setShowTagEditor(false)}
          />
        </div>
      )}
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add web/src/components/ThumbnailCell.tsx
git commit -m "feat(web): add ThumbnailCell component with hover overlay, inline rename, tag popover"
```

---

### Task 7: Add ThumbnailCell CSS

**Files:**
- Modify: `web/src/styles/terminal.css` (add after the existing mini-preview section around line 1314)

**Step 1: Add thumbnail grid and cell CSS**

In `web/src/styles/terminal.css`, add before the `/* Session list (below preview) */` comment (line 1316):

```css
/* Thumbnail grid */
.thumb-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(0, 1fr));
  gap: 4px;
  margin-top: 6px;
}

/* Ensure minimum 2 columns */
.thumb-grid {
  grid-template-columns: repeat(auto-fill, minmax(calc(50% - 2px), 1fr));
}

.thumb-cell {
  position: relative;
  aspect-ratio: 4 / 3;
  border-radius: 4px;
  overflow: hidden;
  background: rgba(0, 0, 0, 0.25);
  cursor: pointer;
  transition: box-shadow 0.15s;
}

.thumb-cell:hover {
  box-shadow: 0 0 0 1px var(--accent, #666);
}

.thumb-cell.focused {
  box-shadow: 0 0 0 1px var(--accent, #666);
}

.thumb-preview {
  position: absolute;
  inset: 0;
  overflow: hidden;
}

/* Status dot — resting state, lower-right */
.thumb-status-dot {
  position: absolute;
  bottom: 4px;
  right: 4px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  z-index: 1;
}

/* Hover overlay — bottom bar */
.thumb-overlay {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 6px;
  background: rgba(0, 0, 0, 0.65);
  z-index: 2;
}

.thumb-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 9px;
  color: var(--fg, #ccc);
  cursor: text;
}

.thumb-name:hover {
  text-decoration: underline;
}

.thumb-rename-input {
  flex: 1;
  min-width: 0;
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid var(--accent, #666);
  border-radius: 2px;
  padding: 1px 4px;
  font-size: 9px;
  color: var(--fg, #ccc);
  outline: none;
  font-family: inherit;
}

/* Tag button — upper-right on hover */
.thumb-tag-btn {
  position: absolute;
  top: 3px;
  right: 3px;
  background: rgba(0, 0, 0, 0.6);
  border: none;
  color: var(--fg, #ccc);
  opacity: 0.7;
  cursor: pointer;
  padding: 1px 4px;
  font-size: 10px;
  line-height: 1;
  border-radius: 3px;
  z-index: 2;
  transition: opacity 0.1s;
}

.thumb-tag-btn:hover {
  opacity: 1;
}

/* Tag popover — anchored to upper-right */
.thumb-tag-popover {
  position: absolute;
  top: 20px;
  right: 3px;
  z-index: 200;
}

.thumb-tag-popover .tag-editor {
  position: relative;
  top: auto;
  left: auto;
  right: auto;
  min-width: 160px;
}

/* Collapsed group header chips */
.sidebar-status-chips {
  display: flex;
  gap: 4px;
  align-items: center;
}

.sidebar-status-chip {
  font-size: 9px;
  font-weight: 600;
  border-radius: 8px;
  padding: 1px 5px;
  min-width: 14px;
  text-align: center;
  line-height: 1.4;
  color: var(--bg, #000);
}

.sidebar-status-chip.idle {
  background: #4ade80;
}

.sidebar-status-chip.running {
  background: #fbbf24;
}

/* Group collapse chevron */
.sidebar-group-chevron {
  font-size: 8px;
  color: var(--fg, #ccc);
  opacity: 0.5;
  transition: transform 0.15s;
  flex-shrink: 0;
  width: 10px;
  text-align: center;
}

.sidebar-group-chevron.expanded {
  transform: rotate(90deg);
}
```

**Step 2: Commit**

```bash
git add web/src/styles/terminal.css
git commit -m "feat(web): add CSS for thumbnail grid, cells, hover overlay, status chips, chevrons"
```

---

### Task 8: Update TagEditor to support Tab/comma/space commit

Currently TagEditor only commits on Enter. Add Tab, comma, and space as tag commit keys.

**Files:**
- Modify: `web/src/components/TagEditor.tsx:84-89` (handleKeyDown)

**Step 1: Update handleKeyDown**

In `web/src/components/TagEditor.tsx`, replace the `handleKeyDown` callback (lines 84-89):

```tsx
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (input.trim()) {
        addTag(input);
      } else {
        // Enter on empty input dismisses
        onClose();
      }
    } else if (e.key === "Tab" || e.key === "," || e.key === " ") {
      if (input.trim()) {
        e.preventDefault();
        addTag(input);
      }
    }
  }, [input, addTag, onClose]);
```

**Step 2: Commit**

```bash
git add web/src/components/TagEditor.tsx
git commit -m "feat(web): add Tab/comma/space as tag commit keys in TagEditor"
```

---

### Task 9: Rewrite Sidebar.tsx — collapsible groups with thumbnail grid

This is the main task. Replace the group body (preview area + session list + timestamp) with collapsible groups containing the thumbnail grid.

**Files:**
- Modify: `web/src/components/Sidebar.tsx` (full rewrite of expanded group body)

**Step 1: Update imports**

Replace the imports at the top of `web/src/components/Sidebar.tsx`:

```tsx
import { useCallback } from "preact/hooks";
import type { WshClient } from "../api/ws";
import { dragState, dropTargetTag, startSessionDrag, handleGroupDragOver, handleGroupDragLeave, handleGroupDrop, endDrag } from "../hooks/useDragDrop";
import { groups, selectedGroups, collapsedGroups, toggleGroupCollapsed, getGroupStatusCounts } from "../state/groups";
import { connectionState } from "../state/sessions";
import { ThumbnailCell } from "./ThumbnailCell";
import { ThemePicker } from "./ThemePicker";
```

Note: Removed `MiniViewPreview`, `TagEditor`, `sessionStatuses`, `SessionStatus`, and `useState` imports. Removed `statusLabel` and `StatusDot` local functions (they moved to ThumbnailCell).

**Step 2: Remove statusLabel and StatusDot**

Delete lines 16-27 (the `statusLabel` function and `StatusDot` component). These are now in `ThumbnailCell.tsx`.

**Step 3: Rewrite the expanded group body**

Replace the group rendering section (lines 98-168) with:

```tsx
      <div class="sidebar-groups">
        {allGroups.map((g) => {
          const isCollapsed = collapsedGroups.value.has(g.tag);
          const { running, idle } = getGroupStatusCounts(g);
          const sortedSessions = [...g.sessions].sort();

          return (
            <div
              key={g.tag}
              class={`sidebar-group ${selected.includes(g.tag) ? "selected" : ""} ${dropTarget === g.tag ? "drop-target" : ""}`}
              onClick={(e: MouseEvent) => handleGroupClick(g.tag, e)}
              onDragOver={(e: DragEvent) => handleGroupDragOver(g.tag, e)}
              onDragLeave={handleGroupDragLeave}
              onDrop={(e: DragEvent) => handleGroupDrop(g.tag, e, client)}
              role="button"
              aria-label={`Group: ${g.label}, ${g.sessions.length} sessions`}
              aria-selected={selected.includes(g.tag)}
            >
              <div class="sidebar-group-header">
                <span
                  class={`sidebar-group-chevron ${isCollapsed ? "" : "expanded"}`}
                  onClick={(e: MouseEvent) => { e.stopPropagation(); toggleGroupCollapsed(g.tag); }}
                >
                  &#9656;
                </span>
                <span class="sidebar-group-label">{g.label}</span>
                {isCollapsed && g.sessions.length > 0 && (
                  <div class="sidebar-status-chips">
                    {idle > 0 && <span class="sidebar-status-chip idle">{idle}</span>}
                    {running > 0 && <span class="sidebar-status-chip running">{running}</span>}
                  </div>
                )}
                {!isCollapsed && (
                  <span class="sidebar-group-count">{g.sessions.length}</span>
                )}
              </div>
              {!isCollapsed && sortedSessions.length > 0 && (
                <div class="thumb-grid">
                  {sortedSessions.map((s) => (
                    <ThumbnailCell key={s} session={s} client={client} />
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
```

**Step 4: Remove editingSession state**

In the component body, remove the `editingSession` useState and `statuses` line since they're no longer used:

```tsx
// Remove these lines:
// const statuses = sessionStatuses.value;
// const [editingSession, setEditingSession] = useState<string | null>(null);
```

**Step 5: Update the sr-only accessibility section**

Remove the old sr-only section (lines 193-200) that referenced `statusLabel`. Replace with:

```tsx
      <div class="sr-only" aria-live="polite">
        {allGroups.map((g) => {
          const { running, idle } = getGroupStatusCounts(g);
          return `${g.label}: ${running} running, ${idle} idle`;
        }).join(". ")}
      </div>
```

**Step 6: Commit**

```bash
git add web/src/components/Sidebar.tsx
git commit -m "feat(web): rewrite sidebar with collapsible groups and thumbnail grid"
```

---

### Task 10: Update queue view labels — "Pending" → "Idle", "Active" → "Running"

**Files:**
- Modify: `web/src/components/QueueView.tsx:103-104,117-118`
- Modify: `web/src/components/MiniViewPreview.tsx:110`

**Step 1: Update QueueView labels**

In `web/src/components/QueueView.tsx`:

Line 104 — change `Pending ({pending.length})` to `Idle ({pending.length})`:
```tsx
          <span class="queue-section-label">Idle ({pending.length})</span>
```

Line 118 — change `Active ({active.length + handled.length})` to `Running ({active.length + handled.length})`:
```tsx
          <span class="queue-section-label">Running ({active.length + handled.length})</span>
```

**Step 2: Update MiniViewPreview mini-queue label**

In `web/src/components/MiniViewPreview.tsx:110`, change:
```tsx
        <span class="mini-queue-count">{pending.length} idle</span>
```

**Step 3: Commit**

```bash
git add web/src/components/QueueView.tsx web/src/components/MiniViewPreview.tsx
git commit -m "fix(web): rename queue labels — 'Pending' → 'Idle', 'Active' → 'Running'"
```

---

### Task 11: Clean up removed CSS

Remove CSS for components that no longer exist in the sidebar: `.sidebar-session-list`, `.sidebar-session-item`, `.sidebar-session-name`, `.sidebar-group-timestamp`, `.tag-edit-btn`, and the old `.sidebar-preview-area`.

**Files:**
- Modify: `web/src/styles/terminal.css`

**Step 1: Remove dead CSS rules**

Delete these CSS blocks from `web/src/styles/terminal.css`:

- `.sidebar-preview-area` (lines 1187-1195)
- `.sidebar-session-list` (lines 1317-1319)
- `.sidebar-session-item` (lines 1321-1332)
- `.sidebar-session-item:hover` (lines 1334-1337)
- `.sidebar-session-name` (lines 1339-1344)
- `.sidebar-session-item:active` (lines 1346-1348)
- `.sidebar-group-timestamp` (lines 1381-1386)
- `.tag-edit-btn` (lines 1569-1583)
- `.sidebar-session-item:hover .tag-edit-btn` (lines 1585-1587)
- `.tag-edit-btn:hover` (lines 1589-1591)
- `.status-dot-grey` (already removed in Task 2, verify)

Also remove the old mini-view mode previews that are no longer used in the sidebar:
- `.mini-carousel` (lines 1229-1235)
- `.mini-carousel-thumb` (lines 1237-1244)
- `.mini-carousel-thumb.active` (lines 1246-1250)
- `.mini-grid` (lines 1253-1258)
- `.mini-grid-row` (lines 1260-1265)
- `.mini-grid-cell` (lines 1267-1273)
- `.mini-queue` (lines 1276-1281)
- `.mini-queue-bar` (lines 1283-1288)
- `.mini-queue-count` (lines 1290-1292)
- `.mini-queue-current` (lines 1294-1301)
- `.mini-queue-others` (lines 1303-1306)
- `.mini-queue-thumb` (lines 1308-1314)

**IMPORTANT**: Before deleting the mini-carousel/grid/queue CSS, verify whether `MiniViewPreview.tsx` is still imported anywhere besides the old Sidebar. Check:
- `QueueView.tsx` imports `MiniTermContent` from `MiniViewPreview` (line 6) — keep `MiniTermContent` and its CSS (`.mini-term-content`, `.mini-term-inner`, `.mini-term-line`)
- The mini carousel/grid/queue CSS is only used by `MiniViewPreview`'s sub-components. Check if `MiniViewPreview` itself is still used anywhere. If QueueView only uses `MiniTermContent`, then the `MiniCarousel`, `MiniGrid`, `MiniQueue` components and their CSS can be removed.

**Step 2: Commit**

```bash
git add web/src/styles/terminal.css
git commit -m "chore(web): remove CSS for replaced sidebar components"
```

---

### Task 12: Clean up MiniViewPreview — remove unused sub-components

After Task 11 verification, if `MiniViewPreview` (the main export) is no longer used, remove the unused sub-components from the file. Keep `MiniTermContent` since `QueueView` and `ThumbnailCell` use it.

**Files:**
- Modify: `web/src/components/MiniViewPreview.tsx`

**Step 1: Check all imports of MiniViewPreview**

```bash
grep -r "MiniViewPreview\|MiniTermContent" web/src/
```

If `MiniViewPreview` is only imported in the old Sidebar (which we rewrote), remove the `MiniCarousel`, `MiniGrid`, `MiniQueue`, and `MiniViewPreview` functions. Keep only `MiniTermContent` and its helper `lineToText`.

**Step 2: Remove unused components**

Delete `MiniCarousel` (lines 63-76), `MiniGrid` (lines 79-99), `MiniQueue` (lines 101-126), and `MiniViewPreview` (lines 128-146) from `MiniViewPreview.tsx`. Also remove the unused imports (`getViewModeForGroup`, `quiescenceQueues`, `focusedSession`, `Group` type).

**Step 3: Commit**

```bash
git add web/src/components/MiniViewPreview.tsx
git commit -m "chore(web): remove unused MiniCarousel/MiniGrid/MiniQueue from MiniViewPreview"
```

---

### Task 13: Build and verify

**Step 1: Run the build**

```bash
cd web && npm run build
```

Fix any TypeScript or build errors.

**Step 2: Manual smoke test**

Start the app and verify:
- Sidebar shows groups with thumbnail grids
- Hovering a thumbnail shows bottom bar with name + status dot
- Clicking name enters inline rename
- Tag icon appears on hover, clicking opens popover
- Tab/comma/space commit tags
- Groups collapse/expand with chevron
- Collapsed groups show colored count chips
- "All Sessions" appears last
- Status dots: green=idle, amber=running
- No "exited" artifacts anywhere

**Step 3: Commit any fixes**

```bash
git add -A
git commit -m "fix(web): address build and integration issues from sidebar redesign"
```

---

### Task 14: Update drag-and-drop for thumbnail cells

The current drag-and-drop allows dragging session items between groups. With the session list removed, thumbnails need to be draggable.

**Files:**
- Modify: `web/src/components/ThumbnailCell.tsx` (add draggable)

**Step 1: Add drag handlers to ThumbnailCell**

In `ThumbnailCell.tsx`, add drag props to the outer div:

```tsx
import { startSessionDrag, endDrag } from "../hooks/useDragDrop";

// In the component JSX, add to the thumb-cell div:
draggable
onDragStart={(e: DragEvent) => startSessionDrag(session, e)}
onDragEnd={endDrag}
```

**Step 2: Commit**

```bash
git add web/src/components/ThumbnailCell.tsx
git commit -m "feat(web): make thumbnail cells draggable for tag-by-drag"
```
