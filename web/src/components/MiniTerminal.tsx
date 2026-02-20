import { getScreenSignal } from "../state/terminal";
import type { FormattedLine } from "../api/types";

interface MiniTerminalProps {
  session: string;
}

export function MiniTerminal({ session }: MiniTerminalProps) {
  const screen = getScreenSignal(session).value;
  // Show up to 8 lines of the current screen (plain text, no styled spans for now)
  return (
    <div class="mini-terminal">
      {screen.lines.slice(0, 8).map((line: FormattedLine, i: number) => (
        <div key={i} class="mini-term-line">
          {typeof line === "string" ? line : line.map((s) => s.text).join("")}
        </div>
      ))}
    </div>
  );
}
