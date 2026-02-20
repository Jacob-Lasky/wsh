import { useCallback } from "preact/hooks";
import type { WshClient } from "../api/ws";
import { groups, selectedGroups, sessionStatuses, type SessionStatus } from "../state/groups";
import { connectionState } from "../state/sessions";
import { MiniTerminal } from "./MiniTerminal";

interface SidebarProps {
  client: WshClient;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

function StatusDot({ status }: { status: SessionStatus | undefined }) {
  const cls = status === "quiescent" ? "status-dot-amber"
    : status === "exited" ? "status-dot-grey"
    : "status-dot-green";
  return <span class={`mini-status-dot ${cls}`} />;
}

export function Sidebar({ client, collapsed, onToggleCollapse }: SidebarProps) {
  const allGroups = groups.value;
  const selected = selectedGroups.value;
  const connState = connectionState.value;
  const statuses = sessionStatuses.value;

  const handleGroupClick = useCallback((tag: string, e: MouseEvent) => {
    if (e.ctrlKey || e.metaKey) {
      const current = selectedGroups.value;
      if (current.includes(tag)) {
        // Prevent deselecting the last group
        const filtered = current.filter((t) => t !== tag);
        if (filtered.length > 0) {
          selectedGroups.value = filtered;
        }
      } else {
        selectedGroups.value = [...current, tag];
      }
    } else {
      selectedGroups.value = [tag];
    }
  }, []);

  const handleNewSession = useCallback(() => {
    client.createSession().catch((e) => {
      console.error("Failed to create session:", e);
    });
  }, [client]);

  if (collapsed) {
    return (
      <div class="sidebar-collapsed">
        <button class="sidebar-expand-btn" onClick={onToggleCollapse} title="Expand sidebar">
          &#9656;
        </button>
        {allGroups.map((g) => (
          <div
            key={g.tag}
            class={`sidebar-icon ${selected.includes(g.tag) ? "active" : ""}`}
            onClick={(e: MouseEvent) => handleGroupClick(g.tag, e)}
            title={`${g.label} (${g.sessions.length})`}
          >
            <span class="sidebar-icon-count">{g.sessions.length}</span>
            {g.badgeCount > 0 && <span class="sidebar-badge">{g.badgeCount}</span>}
          </div>
        ))}
        <div style={{ flex: 1 }} />
        <div class={`status-dot ${connState}`} title={connState} />
        <button class="sidebar-icon sidebar-new-icon" onClick={handleNewSession} title="New session">
          +
        </button>
      </div>
    );
  }

  return (
    <div class="sidebar-content">
      <div class="sidebar-header">
        <span class="sidebar-title">Sessions</span>
        <button class="sidebar-collapse-btn" onClick={onToggleCollapse} title="Collapse sidebar">
          &#9666;
        </button>
      </div>
      <div class="sidebar-groups">
        {allGroups.map((g) => (
          <div
            key={g.tag}
            class={`sidebar-group ${selected.includes(g.tag) ? "selected" : ""}`}
            onClick={(e: MouseEvent) => handleGroupClick(g.tag, e)}
          >
            <div class="sidebar-group-header">
              <span class="sidebar-group-label">{g.label}</span>
              <span class="sidebar-group-count">{g.sessions.length}</span>
              {g.badgeCount > 0 && <span class="sidebar-badge">{g.badgeCount}</span>}
            </div>
            {/* Mini-preview grid: up to 4 sessions in 2x2 */}
            {g.sessions.length > 0 && (
              <div class="sidebar-preview-grid">
                {g.sessions.slice(0, 4).map((s) => (
                  <div key={s} class="sidebar-preview-cell">
                    <MiniTerminal session={s} />
                    <StatusDot status={statuses.get(s)} />
                  </div>
                ))}
              </div>
            )}
            {/* Timestamp */}
            {g.sessions.length > 0 && (
              <div class="sidebar-group-timestamp">
                {(() => {
                  // Show "Last active" based on group activity
                  const hasQuiescent = g.sessions.some((s) => statuses.get(s) === "quiescent");
                  const allExited = g.sessions.every((s) => statuses.get(s) === "exited");
                  if (allExited) return "Exited";
                  if (hasQuiescent) return "Idle";
                  return "Active";
                })()}
              </div>
            )}
          </div>
        ))}
      </div>
      <div class="sidebar-footer">
        <div class={`status-dot ${connState}`} title={connState} />
        <button class="sidebar-new-session-btn" onClick={handleNewSession} title="New session">
          + New
        </button>
      </div>
    </div>
  );
}
