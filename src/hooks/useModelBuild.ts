import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useToast } from "../Toast";
import type { ModelStorage } from "./useModelStorage";

type AgentEvent =
  | { kind: "message"; text: string }
  | { kind: "thought"; text: string }
  | { kind: "toolCall"; id: string; name: string; status: string }
  | { kind: "plan"; content: string }
  | { kind: "activity" }
  | { kind: "completed"; stopReason: string }
  | { kind: "failed"; error: string }
  | { kind: "cancelled" };

export interface ModelBuild {
  /** A full auto-context build is in progress. */
  building: boolean;
  /** A semantic drift check is in progress. */
  checking: boolean;
  /** Either kind of agent run is in progress. */
  active: boolean;
  /** Stable "where the agent is" marker (e.g. which container) — survives the
   * noisy per-line stream. Set from the orchestrator's "▶ …" phase messages. */
  phase: string | null;
  /** Ids of the nodes the agent is generating right now — the canvas rings each.
   *  Wave 2 builds several containers at once, so this is a set, not one id. */
  activeNodeIds: ReadonlySet<string>;
  /** Most recent human-readable line from the stream. */
  activity: string | null;
  /** Kick off the orchestrated build and switch to the (streaming) canvas. */
  start: (cwd: string) => Promise<void>;
  /** Re-check the open model against the code for semantic drift. */
  checkDrift: (cwd: string) => Promise<void>;
  /** Cancel the running build/check. */
  cancel: () => void;
}

/// Drives the orchestrated build (`start_model_build`) and the semantic drift
/// check (`start_drift_check`): suppresses the canvas write-back so the agent
/// owns the file while it streams changes in, and persists the laid-out
/// positions once it finishes. Lives above the picker↔canvas split so its state
/// survives that transition.
export function useModelBuild(storage: ModelStorage): ModelBuild {
  const { toast } = useToast();
  const [building, setBuilding] = useState(false);
  const [checking, setChecking] = useState(false);
  const [phase, setPhase] = useState<string | null>(null);
  const [activity, setActivity] = useState<string | null>(null);
  const [activeNodeIds, setActiveNodeIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const unlisten = useRef<(() => void) | null>(null);
  const unlistenNode = useRef<(() => void) | null>(null);
  const activeRef = useRef(false);
  // Polls the agent's writes from disk onto the canvas while the build runs.
  const pollTimer = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    return () => {
      unlisten.current?.();
      unlistenNode.current?.();
      if (pollTimer.current) clearInterval(pollTimer.current);
    };
  }, []);

  const finish = useCallback(() => {
    unlisten.current?.();
    unlisten.current = null;
    unlistenNode.current?.();
    unlistenNode.current = null;
    if (pollTimer.current) {
      clearInterval(pollTimer.current);
      pollTimer.current = null;
    }
    activeRef.current = false;
    storage.setAgentRunning(false);
    // The agent owns the file during a build; load its authoritative result from
    // disk. Never write our in-memory model back here — if a live reload was
    // missed it is stale/empty and would clobber the agent's work (the model
    // would then survive only in the separate baseline file). autoLayout re-seeds
    // and persists positions for the freshly-loaded nodes now that writes resume.
    void storage.reloadFromDisk();
    setBuilding(false);
    setChecking(false);
    setPhase(null);
    setActivity(null);
    setActiveNodeIds(new Set());
  }, [storage]);

  const run = useCallback(
    async (kind: "build" | "drift", cwd: string) => {
      if (activeRef.current) return;
      activeRef.current = true;
      if (kind === "build") setBuilding(true);
      else setChecking(true);
      setPhase(null);
      setActivity("starting…");
      setActiveNodeIds(new Set());
      storage.setAgentRunning(true);
      // Stream the agent's writes onto the canvas without depending on the OS
      // file watcher (which can miss/coalesce events under a burst of writes):
      // poll the model from disk while the run is active. finish() clears this
      // and does a final reload. No-ops until openProject sets the model ref.
      if (pollTimer.current) clearInterval(pollTimer.current);
      pollTimer.current = setInterval(() => {
        void storage.reloadFromDisk();
      }, 500);

      const label = kind === "build" ? "Model build" : "Drift check";
      const off = await listen<AgentEvent>("agent-event", (event) => {
        const p = event.payload;
        switch (p.kind) {
          case "toolCall":
            setActivity(`→ ${p.name}`);
            break;
          case "message":
          case "thought": {
            const t = p.text.trim();
            if (!t) break;
            // "▶ …" lines are the orchestrator's stable phase markers (which
            // wave / container). Keep them as the persistent location readout;
            // everything else is transient per-line activity.
            if (t.startsWith("▶")) {
              setPhase(t.replace(/^▶\s*/, "").replace(/[…\s]+$/, ""));
            } else {
              setActivity(t);
            }
            break;
          }
          case "plan":
            setActivity("planning…");
            break;
          case "activity":
            setActivity((a) => a ?? "working…");
            break;
          case "failed":
            toast(`${label} failed: ${p.error}`, "error");
            finish();
            break;
          case "completed":
          case "cancelled":
            finish();
            break;
        }
      });
      unlisten.current = off;

      // Which nodes the agent is generating right now — drives the canvas rings.
      // Payload is the full active set (several containers during a parallel
      // Wave 2); an empty array clears all rings.
      const offNode = await listen<string[]>("build-active-node", (event) => {
        setActiveNodeIds(new Set(event.payload ?? []));
      });
      unlistenNode.current = offNode;

      try {
        if (kind === "build") {
          // Creates a blank model (so the canvas can open) then drives the build
          // in the background; resolves once the build has started.
          await invoke("start_model_build", { cwd });
          // Load the now-created model so the canvas takes over and streams.
          await storage.openProject(cwd);
        } else {
          await invoke("start_drift_check", { cwd });
        }
      } catch (e) {
        toast(`${label} failed to start: ${String(e)}`, "error");
        finish();
      }
    },
    [storage, toast, finish],
  );

  const start = useCallback((cwd: string) => run("build", cwd), [run]);
  const checkDrift = useCallback((cwd: string) => run("drift", cwd), [run]);

  const cancel = useCallback(async () => {
    try {
      await invoke("cancel_agent_session");
    } catch {
      /* best-effort — the cancelled event drives teardown */
    }
  }, []);

  return {
    building,
    checking,
    active: building || checking,
    phase,
    activity,
    activeNodeIds,
    start,
    checkDrift,
    cancel,
  };
}
