import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type AgentEvent =
  | { kind: "message"; text: string }
  | { kind: "thought"; text: string }
  | { kind: "toolCall"; id: string; name: string; status: string }
  | { kind: "plan"; content: string }
  | { kind: "activity" }
  | { kind: "completed"; stopReason: string }
  | { kind: "failed"; error: string }
  | { kind: "cancelled" };

export interface AgentSession {
  running: boolean;
  label: string;
  lastTool: string | null;
  startFill: (cwd: string, modelRef: string, nodeId: string, nodeName: string) => void;
  cancel: () => void;
}

export function useAgentSession(): AgentSession {
  const [running, setRunning] = useState(false);
  const [label, setLabel] = useState("");
  const [lastTool, setLastTool] = useState<string | null>(null);
  const unlisten = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      unlisten.current?.();
    };
  }, []);

  const startFill = useCallback(
    (cwd: string, modelRef: string, nodeId: string, nodeName: string) => {
      if (running) return;
      setRunning(true);
      setLabel(nodeName || "node");
      setLastTool(null);

      (async () => {
        const off = await listen<AgentEvent>("agent-event", (event) => {
          const p = event.payload;
          switch (p.kind) {
            case "toolCall":
              setLastTool(p.name);
              break;
            case "completed":
            case "failed":
            case "cancelled":
              unlisten.current?.();
              unlisten.current = null;
              setRunning(false);
              break;
          }
        });
        unlisten.current = off;

        try {
          await invoke<string>("start_node_fill_session", {
            cwd,
            modelRef,
            nodeId,
          });
        } catch {
          unlisten.current?.();
          unlisten.current = null;
          setRunning(false);
        }
      })();
    },
    [running],
  );

  const cancel = useCallback(async () => {
    try {
      await invoke("cancel_agent_session");
    } catch {
      /* best-effort */
    }
  }, []);

  return { running, label, lastTool, startFill, cancel };
}
