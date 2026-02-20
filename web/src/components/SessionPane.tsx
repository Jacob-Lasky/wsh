import { useState, useEffect } from "preact/hooks";
import { Terminal } from "./Terminal";
import { InputBar } from "./InputBar";
import type { WshClient } from "../api/ws";

interface SessionPaneProps {
  session: string;
  client: WshClient;
}

export function SessionPane({ session, client }: SessionPaneProps) {
  const [isMobile, setIsMobile] = useState(false);

  useEffect(() => {
    const mq = window.matchMedia("(pointer: coarse)");
    setIsMobile(mq.matches);
    const handler = (e: MediaQueryListEvent) => setIsMobile(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  return (
    <div class="session-pane">
      <Terminal session={session} client={client} captureInput={!isMobile} />
      {isMobile && <InputBar session={session} client={client} />}
    </div>
  );
}
