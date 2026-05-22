/**
 * Empty-state screen: choose a project to open, or generate a model from a
 * codebase via the configured AI agent.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Sparkles, X, AlertTriangle } from "lucide-react";
import type { ModelStorage } from "./hooks/useModelStorage";
import { useToast } from "./Toast";

type Phase = "picker" | "needs-model" | "generating";

export function ProjectPicker({ storage }: { storage: ModelStorage }) {
  const { toast } = useToast();
  const [phase, setPhase] = useState<Phase>("picker");
  const [generationLog, setGenerationLog] = useState<string[]>([]);
  const agentUnlisten = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (storage.status === "needs-model") {
      setPhase((prev) => (prev === "generating" ? prev : "needs-model"));
    }
    if (storage.status === "ready") {
      stopAgentListener();
    }
  }, [storage.status]);

  useEffect(() => {
    return () => stopAgentListener();
  }, []);

  const stopAgentListener = () => {
    agentUnlisten.current?.();
    agentUnlisten.current = null;
  };

  const pickFolder = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    setPhase("picker");
    await storage.openProject(dir);
  }, [storage]);

  const onCreateBlank = useCallback(async () => {
    if (!storage.projectPath) return;
    await storage.createBlankModel(storage.projectPath);
  }, [storage]);

  const onGenerate = useCallback(async () => {
    if (!storage.projectPath) return;
    const path = storage.projectPath;
    setPhase("generating");
    setGenerationLog([]);

    // Subscribe BEFORE invoking — the runtime emits events as soon as the
    // session starts, and we'd miss them otherwise. The `start_initial_model
    // _session` invoke returns when the agent has spawned (not when it
    // finishes); the actual end-of-session signal is an `agent-event` with
    // kind = "completed" | "failed" | "cancelled".
    type AgentEvent =
      | { kind: "message"; text: string }
      | { kind: "thought"; text: string }
      | { kind: "toolCall"; id: string; name: string; status: string }
      | { kind: "plan"; content: string }
      | { kind: "activity" }
      | { kind: "completed"; stopReason: string }
      | { kind: "failed"; error: string }
      | { kind: "cancelled" };

    const off = await listen<AgentEvent>("agent-event", async (event) => {
      const payload = event.payload;
      // Append a one-line summary to the live log.
      const summary = (() => {
        switch (payload.kind) {
          case "message":
          case "thought":
            return payload.text;
          case "toolCall":
            return `[${payload.status}] ${payload.name}`;
          case "plan":
            return `(plan) ${payload.content}`;
          case "activity":
            return null;
          case "completed":
            return `completed (${payload.stopReason})`;
          case "failed":
            return `failed: ${payload.error}`;
          case "cancelled":
            return "cancelled";
        }
      })();
      if (summary !== null) {
        setGenerationLog((log) => [...log.slice(-400), summary]);
      }

      // Terminal events: tear down and route based on outcome.
      if (
        payload.kind === "completed" ||
        payload.kind === "failed" ||
        payload.kind === "cancelled"
      ) {
        stopAgentListener();
        if (payload.kind === "completed") {
          await storage.openProject(path);
        } else if (payload.kind === "failed") {
          toast(`Generation failed: ${payload.error}`, "error");
          setPhase("needs-model");
        } else {
          setPhase("needs-model");
        }
      }
    });
    agentUnlisten.current = off;

    try {
      // Resolves when the agent has *started* (returns a session id), not
      // when the session ends. We rely on the `agent-event` stream above for
      // termination.
      await invoke<string>("start_initial_model_session", {
        cwd: path,
        modelRef: `project:${path}`,
      });
    } catch (e) {
      stopAgentListener();
      toast(`Generation failed: ${String(e)}`, "error");
      setPhase("needs-model");
    }
  }, [storage, toast]);

  const onCancelGeneration = useCallback(async () => {
    try {
      await invoke("cancel_agent_session");
    } catch {
      /* cancel is best-effort */
    }
    // Don't tear down the listener yet — the runtime will emit a `cancelled`
    // event which our handler uses to transition phase back to needs-model.
  }, []);

  const onOpenRecent = useCallback(
    async (path: string) => {
      await storage.openProject(path);
    },
    [storage],
  );

  if (storage.status === "legacy") {
    return (
      <Centered>
        <div className="flex flex-col items-center gap-3 max-w-md text-center">
          <AlertTriangle className="h-8 w-8 text-amber-500" />
          <h2 className="text-base font-semibold text-[var(--text)]">
            Legacy model
          </h2>
          <p className="text-sm text-[var(--text-muted)]">{storage.error}</p>
          <button
            type="button"
            className="mt-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-1.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
            onClick={storage.closeProject}
          >
            Pick another project
          </button>
        </div>
      </Centered>
    );
  }

  if (phase === "generating") {
    return (
      <Centered>
        <div className="flex flex-col items-center gap-4 max-w-2xl w-full">
          <div className="flex items-center gap-2 text-[var(--text)]">
            <Sparkles className="h-5 w-5 text-amber-400" />
            <h2 className="text-base font-semibold">
              Generating model from codebase
            </h2>
          </div>
          <p className="text-sm text-[var(--text-muted)] text-center">
            The agent is reading {storage.projectPath} and writing the model via MCP.
            This can take a few minutes.
          </p>
          <div className="w-full max-h-64 overflow-y-auto rounded border border-[var(--border)] bg-[var(--surface-canvas)] p-3 font-mono text-[11px] text-[var(--text-tertiary)]">
            {generationLog.length === 0 ? (
              <span className="text-[var(--text-ghost)]">starting…</span>
            ) : (
              generationLog.map((line, i) => (
                <div key={i} className="whitespace-pre-wrap break-words">
                  {line}
                </div>
              ))
            )}
          </div>
          <button
            type="button"
            onClick={onCancelGeneration}
            className="text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
          >
            Cancel
          </button>
        </div>
      </Centered>
    );
  }

  if (phase === "needs-model" && storage.projectPath) {
    return (
      <Centered>
        <div className="flex flex-col items-center gap-4 max-w-md text-center">
          <h2 className="text-base font-semibold text-[var(--text)]">
            No model in this project yet
          </h2>
          <p className="text-sm text-[var(--text-muted)]">
            {storage.projectPath}
          </p>
          <div className="flex flex-col gap-2 w-full mt-2">
            <button
              type="button"
              className="flex items-center justify-center gap-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-2 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              onClick={onGenerate}
            >
              <Sparkles className="h-4 w-4 text-amber-400" />
              Generate from codebase
              <span className="text-xs text-[var(--text-ghost)]">
                (uses your local AI agent)
              </span>
            </button>
            <button
              type="button"
              className="flex items-center justify-center gap-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-2 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              onClick={onCreateBlank}
            >
              Start blank
            </button>
            <button
              type="button"
              className="text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] mt-1"
              onClick={storage.closeProject}
            >
              Pick a different project
            </button>
          </div>
        </div>
      </Centered>
    );
  }

  return (
    <Centered>
      <div className="flex flex-col gap-6 max-w-md w-full">
        <div>
          <h1 className="text-lg font-semibold text-[var(--text)]">scryer</h1>
          <p className="mt-1 text-sm text-[var(--text-muted)]">
            Open a project to load or build its architecture model.
          </p>
        </div>
        <button
          type="button"
          className="flex items-center justify-center gap-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-4 py-3 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
          onClick={pickFolder}
        >
          <FolderOpen className="h-4 w-4" />
          Open project folder
        </button>
        {storage.recentProjects.length > 0 && (
          <div>
            <h3 className="text-xs font-semibold uppercase tracking-wide text-[var(--text-ghost)] mb-2">
              Recent
            </h3>
            <ul className="flex flex-col gap-1">
              {storage.recentProjects.map((path) => (
                <li
                  key={path}
                  className="group flex items-center justify-between rounded px-2 py-1.5 hover:bg-[var(--surface-hover)]"
                >
                  <button
                    type="button"
                    className="flex-1 text-left text-sm text-[var(--text-secondary)] hover:text-[var(--text)] truncate"
                    onClick={() => onOpenRecent(path)}
                    title={path}
                  >
                    {path}
                  </button>
                  <button
                    type="button"
                    className="ml-2 opacity-0 group-hover:opacity-100 text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
                    onClick={() => storage.forgetRecent(path)}
                    aria-label="Remove from recents"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
        {storage.error && (
          <div className="rounded border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-300">
            {storage.error}
          </div>
        )}
      </div>
    </Centered>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-[var(--surface-canvas)] p-8">
      {children}
    </div>
  );
}
