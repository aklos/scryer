import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useAgentFailure } from "../AgentFailure";

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
  /** How the LAST run ended — `null` while none has, or one is in flight.
   *  Callers reacting to the running→idle edge must branch on this: a failed
   *  run's artifacts don't exist, so treating every falling edge as success
   *  (e.g. flipping a modal to "ready") shows tiles that 404. */
  outcome: "completed" | "failed" | "cancelled" | null;
  /** B5 repair path: write realistic fixture props for a component whose
   *  deterministic preview rendered empty or crashed. */
  startFixture: (
    cwd: string, modelRef: string, nodeId: string, nodeName: string,
    renderStatus: string, renderError: string | null,
  ) => void;
  startVariation: (
    cwd: string, modelRef: string, nodeId: string, nodeName: string,
    prompt: string, variationCount?: number, baseVariationIdx?: number,
  ) => void;
  cancel: () => void;
}

export function useAgentSession(): AgentSession {
  const { report } = useAgentFailure();
  const [running, setRunning] = useState(false);
  const [label, setLabel] = useState("");
  const [lastTool, setLastTool] = useState<string | null>(null);
  const [activity, setActivity] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<AgentSession["outcome"]>(null);
  const unlisten = useRef<(() => void) | null>(null);
  // Synchronous re-entry guard. The `running` STATE is async — two launches in
  // one tick both read false, and the loser's teardown then unhooks the
  // winner's listener. The ref flips before anything async happens.
  const runningRef = useRef(false);

  useEffect(() => {
    return () => {
      unlisten.current?.();
    };
  }, []);

  const startSession = useCallback(
    (command: string, taskLabel: string, args: Record<string, unknown>) => {
      if (runningRef.current) return;
      runningRef.current = true;
      setRunning(true);
      setOutcome(null);
      setLabel(taskLabel);
      setLastTool(null);
      setActivity("starting…");

      const end = (how: NonNullable<AgentSession["outcome"]>) => {
        unlisten.current?.();
        unlisten.current = null;
        runningRef.current = false;
        setOutcome(how);
        setRunning(false);
        setActivity(null);
      };

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
              report({
                title: `${taskLabel} failed`,
                error: p.error,
                consequence: "No changes were made to your model — this run is ephemeral.",
              });
              end("failed");
              break;
            case "completed":
              end("completed");
              break;
            case "cancelled":
              end("cancelled");
              break;
          }
        });
        if (!runningRef.current) {
          // The run already ended (fast failure) before the listener attached.
          off();
          return;
        }
        unlisten.current = off;

        try {
          await invoke<string>(command, args);
        } catch (e) {
          report({
            title: `${taskLabel} failed to start`,
            error: String(e),
            consequence: "Nothing was changed — the run never started.",
          });
          end("failed");
        }
      })();
    },
    [report],
  );

  const startFixture = useCallback(
    (cwd: string, modelRef: string, nodeId: string, nodeName: string, renderStatus: string, renderError: string | null) => {
      startSession(
        "start_preview_fixture_session",
        `Preview data for ${nodeName || "component"}`,
        { cwd, modelRef, nodeId, renderStatus, renderError },
      );
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

  return { running, label, lastTool, activity, outcome, startFixture, startVariation, cancel };
}
