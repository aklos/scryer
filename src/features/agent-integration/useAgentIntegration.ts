import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Where one entry stands. `stale` is scryer's but not what `on` writes now
 *  (a moved binary, an earlier event set); `foreign` is a slot someone else
 *  holds, which scryer leaves alone. */
export type EntryState = "installed" | "missing" | "stale" | "foreign";

export interface SetupEntry {
  part: "mcpServer" | "autoApprove" | "hooks" | "statusline" | "skill" | "gitignore";
  /** Project-relative. */
  file: string;
  state: EntryState;
  note?: string;
}

export interface ToolSetup {
  tool: "claudeCode" | "codex" | "copilot";
  name: string;
  /** On PATH. `on` writes only for detected tools. */
  detected: boolean;
  entries: SetupEntry[];
}

/** `scryer-mcp setup`'s readout: one state per project, and what is
 *  installed per tool, per file. */
export interface SetupStatus {
  on: boolean;
  tools: ToolSetup[];
  gitignore: SetupEntry;
  notes: string[];
}

/** What each part is called where people read it. */
export const PART_LABEL: Record<SetupEntry["part"], string> = {
  mcpServer: "mcp server",
  autoApprove: "auto-approve",
  hooks: "hooks",
  statusline: "status line",
  skill: "skill",
  gitignore: "gitignore",
};

/** What `setup on` would fix in a project scryer was set up in before. */
export interface IntegrationGap {
  /** Scryer's, but not what this scryer writes: an earlier skill, a moved binary. */
  stale: SetupEntry["part"][];
  /** Not there, for a tool `setup on` writes for. */
  missing: SetupEntry["part"][];
}

/** The parts out of date and the parts missing, when some of scryer's
 *  entries are in place but integration is not on: a project set up by an
 *  earlier scryer, or half-undone by hand. Null when it is on, or was never
 *  set up at all. */
export function integrationGap(status: SetupStatus): IntegrationGap | null {
  const all = [status.gitignore, ...status.tools.flatMap((t) => t.entries)];
  if (status.on || !all.some((e) => e.state === "installed" || e.state === "stale")) return null;
  const writable = [status.gitignore, ...status.tools.filter((t) => t.detected).flatMap((t) => t.entries)];
  const parts = (state: EntryState) => [...new Set(writable.filter((e) => e.state === state).map((e) => e.part))];
  return { stale: parts("stale"), missing: parts("missing") };
}

export interface AgentIntegration {
  status: SetupStatus | null;
  busy: boolean;
  error: string | null;
  /** `scryer-mcp setup on|off` for this project. */
  setOn: (on: boolean) => Promise<void>;
  reload: () => void;
}

/// The project's one agent-integration switch, over the same code path as
/// `scryer-mcp setup`.
export function useAgentIntegration(projectPath: string | null): AgentIntegration {
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    if (!projectPath) {
      setStatus(null);
      return;
    }
    invoke<SetupStatus>("agent_integration", { projectPath })
      .then(setStatus)
      .catch(() => setStatus(null));
  }, [projectPath]);
  useEffect(reload, [reload]);

  const setOn = useCallback(
    async (on: boolean) => {
      if (!projectPath) return;
      setBusy(true);
      setError(null);
      try {
        setStatus(await invoke<SetupStatus>("set_agent_integration", { projectPath, on }));
      } catch (e) {
        setError(String(e));
        reload();
      } finally {
        setBusy(false);
      }
    },
    [projectPath, reload],
  );

  return { status, busy, error, setOn, reload };
}
