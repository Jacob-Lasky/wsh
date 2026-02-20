import { useRef, useEffect, useState } from "preact/hooks";
import { getScreenSignal } from "../state/terminal";
import type { FormattedLine } from "../api/types";

/** Extract plain text from a FormattedLine. */
function lineToText(line: FormattedLine): string {
  if (typeof line === "string") return line;
  return line.map((s) => s.text).join("");
}

/**
 * Render a scaled-down replica of the full terminal screen.
 * Renders all visible lines at a base font size, then uses CSS
 * transform to scale down to fit the container.
 */
export function MiniTermContent({ session }: { session: string }) {
  const screen = getScreenSignal(session).value;
  const containerRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);

  useEffect(() => {
    const container = containerRef.current;
    const inner = innerRef.current;
    if (!container || !inner) return;

    const ro = new ResizeObserver(() => {
      const cw = container.clientWidth;
      const ch = container.clientHeight;
      const iw = inner.scrollWidth;
      const ih = inner.scrollHeight;
      if (iw > 0 && ih > 0) {
        setScale(Math.min(cw / iw, ch / ih, 1));
      }
    });
    ro.observe(container);
    return () => ro.disconnect();
  }, [screen.lines.length]);

  return (
    <div class="mini-term-content" ref={containerRef}>
      <div
        class="mini-term-inner"
        ref={innerRef}
        style={{ transform: `scale(${scale})`, transformOrigin: "top left" }}
      >
        {screen.lines.map((line: FormattedLine, i: number) => (
          <div key={i} class="mini-term-line">{lineToText(line)}</div>
        ))}
      </div>
    </div>
  );
}
