import { createConnection } from "node:net";
import { writeFileSync } from "node:fs";

type BridgeEvent =
  | "session_start"
  | "agent_start"
  | "agent_settled"
  | "ui_prompt_start"
  | "ui_prompt_end"
  | "session_shutdown";

interface ExtensionAPI {
  on(event: BridgeEvent, handler: () => void): void;
}

export default function deeppiBridge(pi: ExtensionAPI) {
  const stateFile = process.env.DEEPPI_STATE_FILE;
  const pipeName = process.env.DEEPPI_BRIDGE_PIPE;

  const writeFallback = (status: "running" | "waiting") => {
    if (!stateFile) return;
    try {
      writeFileSync(stateFile, status, "utf8");
    } catch {
      // Status reporting is best-effort and must never interrupt Pi.
    }
  };

  const report = (status: "running" | "waiting") => {
    if (!pipeName) {
      writeFallback(status);
      return;
    }

    let delivered = false;
    const socket = createConnection(pipeName);
    const fallback = () => {
      if (!delivered) {
        delivered = true;
        writeFallback(status);
      }
    };
    socket.setTimeout(250, () => {
      fallback();
      socket.destroy();
    });
    socket.once("connect", () => {
      delivered = true;
      socket.end(`${status}\n`);
    });
    socket.once("error", fallback);
  };

  pi.on("session_start", () => report("waiting"));
  pi.on("agent_start", () => report("running"));
  pi.on("agent_settled", () => report("waiting"));
  pi.on("ui_prompt_start", () => report("waiting"));
  pi.on("ui_prompt_end", () => report("running"));
  pi.on("session_shutdown", () => report("waiting"));
}
