import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useToast } from "../Toast";

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
  /** Most recent human-readable line from the agent stream (tool calls,
   *  message snippets, status) — drives the live activity readout. */
  activity: string | null;
  startFill: (cwd: string, modelRef: string, nodeId: string, nodeName: string) => void;
  startPreview: (cwd: string, modelRef: string, nodeId: string, nodeName: string) => void;
  startVariation: (
    cwd: string, modelRef: string, nodeId: string, nodeName: string,
    prompt: string, variationCount?: number, baseVariationIdx?: number,
  ) => void;
  cancel: () => void;
}

export function useAgentSession(): AgentSession {
  const { toast } = useToast();
  const [running, setRunning] = useState(false);
  const [label, setLabel] = useState("");
  const [lastTool, setLastTool] = useState<string | null>(null);
  const [activity, setActivity] = useState<string | null>(null);
  const unlisten = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      unlisten.current?.();
    };
  }, []);

  const startSession = useCallback(
    (command: string, taskLabel: string, args: Record<string, unknown>) => {
      if (running) return;
      setRunning(true);
      setLabel(taskLabel);
      setLastTool(null);
      setActivity("starting…");

      (async () => {
        const off = await listen<AgentEvent>("agent-event", (event) => {
          const p = event.payload;
          switch (p.kind) {
            case "toolCall":
              setLastTool(p.name);
              setActivity(`→ ${p.name}`);
              break;
            case "message":
            case "thought":
              if (p.text.trim()) setActivity(p.text.trim());
              break;
            case "plan":
              setActivity("planning…");
              break;
            case "activity":
              setActivity((a) => a ?? "working…");
              break;
            case "failed":
              toast(`${taskLabel} failed: ${p.error}`, "error");
              unlisten.current?.();
              unlisten.current = null;
              setRunning(false);
              setActivity(null);
              break;
            case "completed":
            case "cancelled":
              unlisten.current?.();
              unlisten.current = null;
              setRunning(false);
              setActivity(null);
              break;
          }
        });
        unlisten.current = off;

        try {
          await invoke<string>(command, args);
        } catch (e) {
          toast(`${taskLabel} failed to start: ${String(e)}`, "error");
          unlisten.current?.();
          unlisten.current = null;
          setRunning(false);
          setActivity(null);
        }
      })();
    },
    [running, toast],
  );

  const startFill = useCallback(
    (cwd: string, modelRef: string, nodeId: string, nodeName: string) => {
      startSession("start_node_fill_session", `Filling ${nodeName || "node"}`, { cwd, modelRef, nodeId });
    },
    [startSession],
  );

  const startPreview = useCallback(
    (cwd: string, modelRef: string, nodeId: string, nodeName: string) => {
      startSession("start_preview_session", `Rendering ${nodeName || "component"}`, { cwd, modelRef, nodeId });
    },
    [startSession],
  );

  const startVariation = useCallback(
    (cwd: string, modelRef: string, nodeId: string, nodeName: string, prompt: string, variationCount?: number, baseVariationIdx?: number) => {
      startSession(
        "start_visual_variation_session",
        `Variations for ${nodeName || "component"}`,
        { cwd, modelRef, nodeId, prompt, variationCount: variationCount ?? null, baseVariationIdx: baseVariationIdx ?? null },
      );
    },
    [startSession],
  );

  const cancel = useCallback(async () => {
    try {
      await invoke("cancel_agent_session");
    } catch {
      /* best-effort */
    }
  }, []);

  return { running, label, lastTool, activity, startFill, startPreview, startVariation, cancel };
}
