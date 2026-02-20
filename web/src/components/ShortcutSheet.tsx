import { useState, useRef, useEffect } from "preact/hooks";

interface Shortcut {
  keys: string;
  description: string;
}

interface ShortcutCategory {
  label: string;
  shortcuts: Shortcut[];
}

const CATEGORIES: ShortcutCategory[] = [
  {
    label: "Navigation",
    shortcuts: [
      { keys: "Super+Left/Right", description: "Carousel rotate" },
      { keys: "Super+1-9", description: "Jump to Nth session" },
      { keys: "Super+Tab", description: "Next sidebar group" },
      { keys: "Super+Shift+Tab", description: "Previous sidebar group" },
      { keys: "Super+Arrow keys", description: "Move focus between tiles" },
    ],
  },
  {
    label: "View Modes",
    shortcuts: [
      { keys: "Super+F", description: "Carousel mode" },
      { keys: "Super+G", description: "Tiled mode" },
      { keys: "Super+Q", description: "Queue mode" },
    ],
  },
  {
    label: "Session Management",
    shortcuts: [
      { keys: "Super+N", description: "New session" },
      { keys: "Super+W", description: "Kill focused session" },
      { keys: "Super+Enter", description: "Dismiss queue item" },
    ],
  },
  {
    label: "UI",
    shortcuts: [
      { keys: "Super+B", description: "Toggle sidebar" },
      { keys: "Super+T", description: "Theme picker" },
      { keys: "Super+K", description: "Command palette" },
      { keys: "Super+?", description: "This help" },
    ],
  },
];

interface ShortcutSheetProps {
  onClose: () => void;
}

export function ShortcutSheet({ onClose }: ShortcutSheetProps) {
  const [filter, setFilter] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [onClose]);

  const lower = filter.toLowerCase();
  const filteredCategories = CATEGORIES.map((cat) => ({
    ...cat,
    shortcuts: cat.shortcuts.filter(
      (s) =>
        s.keys.toLowerCase().includes(lower) ||
        s.description.toLowerCase().includes(lower)
    ),
  })).filter((cat) => cat.shortcuts.length > 0);

  return (
    <div class="shortcut-backdrop" onClick={onClose}>
      <div class="shortcut-sheet" ref={containerRef} onClick={(e: MouseEvent) => e.stopPropagation()}>
        <div class="shortcut-sheet-header">
          <span class="shortcut-sheet-title">Keyboard Shortcuts</span>
          <button class="shortcut-sheet-close" onClick={onClose}>&times;</button>
        </div>
        <input
          class="shortcut-filter"
          type="text"
          placeholder="Filter shortcuts..."
          value={filter}
          onInput={(e) => setFilter((e.target as HTMLInputElement).value)}
          autoFocus
        />
        <div class="shortcut-categories">
          {filteredCategories.map((cat) => (
            <div key={cat.label} class="shortcut-category">
              <div class="shortcut-category-label">{cat.label}</div>
              {cat.shortcuts.map((s) => (
                <div key={s.keys} class="shortcut-row">
                  <kbd class="shortcut-keys">{s.keys}</kbd>
                  <span class="shortcut-desc">{s.description}</span>
                </div>
              ))}
            </div>
          ))}
          {filteredCategories.length === 0 && (
            <div class="shortcut-empty">No matching shortcuts</div>
          )}
        </div>
        <div class="shortcut-footer">
          Tip: Super = Cmd on Mac, Win on Windows. Fallback: Ctrl+Shift
        </div>
      </div>
    </div>
  );
}
