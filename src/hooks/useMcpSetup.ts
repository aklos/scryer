import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Full per-project AI-tool state from `detect_ai_tools`: which CLIs are on
 *  PATH, and whether THIS project already has scryer wired into each one's
 *  config. The `*Enabled` / `*Approved` flags are always false when no project
 *  path is given (the PATH-only check used by the launch readout elsewhere). */
export interface AiToolsState {
  claude: boolean;
  codex: boolean;
  claudeMcpEnabled: boolean;
  codexMcpEnabled: boolean;
  claudeReadApproved: boolean;
}

const EMPTY: AiToolsState = {
  claude: false,
  codex: false,
  claudeMcpEnabled: false,
  codexMcpEnabled: false,
  claudeReadApproved: false,
};

export interface McpSetup {
  tools: AiToolsState;
  /** A detected agent exists whose scryer MCP config this project is missing —
   *  the signal to offer setup. Read auto-approve alone never nags (it rides
   *  along in `enable`, but its absence isn't worth a prompt). */
  needsSetup: boolean;
  /** The user clicked "Not now" for this project this session. */
  dismissed: boolean;
  /** An enable write is in flight. */
  busy: boolean;
  /** Write every applicable config — `.mcp.json`, `.codex/config.toml`, and
   *  read auto-approve in `.claude/settings.local.json` — then re-detect. */
  enable: () => Promise<void>;
  dismiss: () => void;
  /** Re-read detection from disk (e.g. after a config is written externally). */
  reload: () => void;
}

/// Detects whether the opened project is wired for AI-tool integration and
/// drives the one-click setup. Backs the post-open enable prompt (already-modeled
/// projects) and the new-project setup screen, which share this so the offer and
/// the writes can never disagree. Dismissal is session-only and per project: a
/// restart re-offers, and silencing one project doesn't silence another.
export function useMcpSetup(projectPath: string | null): McpSetup {
  const [tools, setTools] = useState<AiToolsState>(EMPTY);
  const [busy, setBusy] = useState(false);
  const [dismissedPaths, setDismissedPaths] = useState<Set<string>>(() => new Set());

  const reload = useCallback(() => {
    if (!projectPath) {
      setTools(EMPTY);
      return;
    }
    invoke<AiToolsState>("detect_ai_tools", { projectPath })
      .then((d) => setTools({ ...EMPTY, ...d }))
      .catch(() => setTools(EMPTY));
  }, [projectPath]);
  useEffect(reload, [reload]);

  const enable = useCallback(async () => {
    if (!projectPath) return;
    setBusy(true);
    try {
      // Only write what's actually missing, so re-enabling is a no-op rather
      // than churning files. Each command merges into existing config.
      const actions: string[] = [];
      if (tools.claude && !tools.claudeMcpEnabled) actions.push("mcp");
      if (tools.codex && !tools.codexMcpEnabled) actions.push("mcp_codex");
      if (tools.claude && !tools.claudeReadApproved) actions.push("claude_read_approve");
      for (const action of actions) {
        await invoke("setup_mcp_integration", { action, projectPath });
      }
      reload();
    } finally {
      setBusy(false);
    }
  }, [projectPath, tools, reload]);

  const dismiss = useCallback(() => {
    if (!projectPath) return;
    setDismissedPaths((prev) => new Set(prev).add(projectPath));
  }, [projectPath]);

  const needsSetup =
    (tools.claude && !tools.claudeMcpEnabled) || (tools.codex && !tools.codexMcpEnabled);
  const dismissed = projectPath ? dismissedPaths.has(projectPath) : false;

  return { tools, needsSetup, dismissed, busy, enable, dismiss, reload };
}
