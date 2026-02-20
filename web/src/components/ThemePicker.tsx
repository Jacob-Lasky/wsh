import { useState, useRef, useEffect } from "preact/hooks";
import { theme, setTheme, type Theme } from "../state/sessions";

const THEMES: { id: Theme; label: string; swatches: string[] }[] = [
  { id: "glass", label: "Glass", swatches: ["#1a1a2e", "#e0e0e8", "#6c63ff", "#44d7b6", "#888"] },
  { id: "neon", label: "Neon", swatches: ["#05050a", "#00ffcc", "#ff2d95", "#00aaff", "#ffdd00"] },
  { id: "minimal", label: "Minimal", swatches: ["#161618", "#c8c8cc", "#f5f5f7", "#81c784", "#64b5f6"] },
  { id: "tokyo-night", label: "Tokyo Night", swatches: ["#1a1b26", "#a9b1d6", "#7aa2f7", "#9ece6a", "#f7768e"] },
  { id: "catppuccin", label: "Catppuccin", swatches: ["#1e1e2e", "#cdd6f4", "#cba6f7", "#a6e3a1", "#f38ba8"] },
  { id: "dracula", label: "Dracula", swatches: ["#282a36", "#f8f8f2", "#bd93f9", "#50fa7b", "#ff79c6"] },
];

export function ThemePicker() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const current = theme.value;

  // Close on click outside
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  return (
    <div class="theme-picker" ref={ref}>
      <button class="theme-picker-btn" onClick={() => setOpen(!open)} title="Change theme">
        &#9673;
      </button>
      {open && (
        <div class="theme-picker-menu">
          {THEMES.map((t) => (
            <button
              key={t.id}
              class={`theme-picker-option ${current === t.id ? "active" : ""}`}
              onClick={() => { setTheme(t.id); setOpen(false); }}
            >
              <div class="theme-swatches">
                {t.swatches.map((color, i) => (
                  <span key={i} class="theme-swatch" style={{ background: color }} />
                ))}
              </div>
              <span class="theme-name">{t.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
