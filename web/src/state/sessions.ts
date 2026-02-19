import { signal } from "@preact/signals";

export type Theme = "glass" | "neon" | "minimal";

const storedTheme = (localStorage.getItem("wsh-theme") as Theme) || "glass";
const storedAuthToken = localStorage.getItem("wsh-auth-token");
const storedZoom = parseFloat(localStorage.getItem("wsh-zoom") || "1");
const initialZoom = Number.isFinite(storedZoom) ? Math.max(0.5, Math.min(2.0, storedZoom)) : 1.0;

export const sessions = signal<string[]>([]);
export const focusedSession = signal<string | null>(null);
export const sessionOrder = signal<string[]>([]);
export const viewMode = signal<"focused" | "overview" | "tiled">("focused");
export const tileLayout = signal<{
  sessions: string[];
  sizes: number[];
} | null>(null);
export const connectionState = signal<
  "connecting" | "connected" | "disconnected"
>("disconnected");
export const theme = signal<Theme>(storedTheme);
export const tileSelection = signal<string[]>([]);
export const authToken = signal<string | null>(storedAuthToken);
export const authRequired = signal<boolean>(false);
export const authError = signal<string | null>(null);
export const zoomLevel = signal<number>(initialZoom);

export function toggleTileSelection(session: string): void {
  const current = tileSelection.value;
  const idx = current.indexOf(session);
  if (idx >= 0) {
    tileSelection.value = current.filter((s) => s !== session);
  } else {
    tileSelection.value = [...current, session];
  }
}

export function clearTileSelection(): void {
  tileSelection.value = [];
}

export function cycleTheme(): Theme {
  const order: Theme[] = ["glass", "neon", "minimal"];
  const idx = order.indexOf(theme.value);
  const next = order[(idx + 1) % order.length];
  theme.value = next;
  localStorage.setItem("wsh-theme", next);
  return next;
}

function setZoom(level: number): void {
  const clamped = Math.round(Math.max(0.5, Math.min(2.0, level)) * 10) / 10;
  zoomLevel.value = clamped;
  localStorage.setItem("wsh-zoom", String(clamped));
}

export function zoomIn(): void {
  setZoom(zoomLevel.value + 0.1);
}

export function zoomOut(): void {
  setZoom(zoomLevel.value - 0.1);
}

export function resetZoom(): void {
  setZoom(1.0);
}
