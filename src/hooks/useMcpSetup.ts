import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Full per-project AI-tool state from `detect_ai_tools`: which CLIs are on
 *  PATH, and whether THIS project already has scryer wired into each one's
 *  config. The `*Enabled` / `*Approved` flags are always false when no project
 *  path is given (the PATH-only check used by the launch readout elsewhere). */
export interface AiToolsState {
  claude: boolean;
  codex: boolean;
  cursor: boolean;
  claudeMcpEnabled: boolean;
  codexMcpEnabled: boolean;
  cursorMcpEnabled: boolean;
  /** Cursor CLI grants only Scryer's MCP tools non-interactive access. */
  cursorApproved: boolean;
  /** Cursor CLI is logged in (`agent status` succeeds). */
  cursorAuthenticated: boolean;
  claudeApproved: boolean;
  /** Scryer's session hooks are registered in this project's Claude Code settings. */
  claudeHooksEnabled: boolean;
  /** Scryer's session hooks are registered in this project's `.codex/hooks.json`. */
  codexHooksEnabled: boolean;
  /** Scryer's status one-liner is registered as this project's Claude Code
   *  statusLine — the persistent segment that also works while Scryer is closed. */
  claudeStatuslineEnabled: boolean;
  /** A FOREIGN statusLine holds Claude Code's single slot here. It's never
   *  clobbered, so the UI surfaces it instead of offering an install. */
  claudeStatuslineForeign: boolean;
}

const EMPTY: AiToolsState = {
  claude: false,
  codex: false,
  cursor: false,
  claudeMcpEnabled: false,
  codexMcpEnabled: false,
  cursorMcpEnabled: false,
  cursorApproved: false,
  cursorAuthenticated: false,
  claudeApproved: false,
  claudeHooksEnabled: false,
  codexHooksEnabled: false,
  claudeStatuslineEnabled: false,
  claudeStatuslineForeign: false,
};

export interface McpSetup {
  tools: AiToolsState;
  /** A detected agent exists whose scryer MCP config this project is missing —
   *  the signal to offer setup. Tool auto-approve alone never nags (it rides
   *  along in `enable`, but its absence isn't worth a prompt). */
  needsSetup: boolean;
  /** The user clicked "Not now" for this project this session. */
  dismissed: boolean;
  /** An enable write is in flight. */
  busy: boolean;
  /** Write every applicable MCP config and least-privilege tool approval,
   *  then re-detect. */
  enable: () => Promise<void>;
  /** Explicit, separate opt-in: install scryer's session hooks for one tool —
   *  Claude Code (`.claude/settings.local.json`) or Codex (`.codex/hooks.json`).
   *  Never bundled into `enable` — the hooks change every session's behavior
   *  (while the app is open), so they are only written when the user asks for
   *  exactly that. */
  enableHooks: (tool: "claude" | "codex") => Promise<void>;
  /** Its own opt-in, separate from the session hooks: register scryer's status
   *  one-liner as Claude Code's persistent statusLine. The only integration that
   *  keeps reporting while Scryer is closed (it reads the model off disk), so it
   *  is worth an install of its own rather than riding the app-gated hooks. */
  enableStatusline: () => Promise<void>;
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
      if (tools.cursor && (!tools.cursorMcpEnabled || !tools.cursorApproved))
        actions.push("mcp_cursor");
      if (tools.claude && !tools.claudeApproved) actions.push("claude_approve");
      for (const action of actions) {
        await invoke("setup_mcp_integration", { action, projectPath });
      }
      reload();
    } finally {
      setBusy(false);
    }
  }, [projectPath, tools, reload]);

  const enableHooks = useCallback(
    async (tool: "claude" | "codex") => {
      if (!projectPath) return;
      setBusy(true);
      try {
        const action = tool === "codex" ? "codex_hooks" : "claude_hooks";
        await invoke("setup_mcp_integration", { action, projectPath });
        reload();
      } finally {
        setBusy(false);
      }
    },
    [projectPath, reload],
  );

  const enableStatusline = useCallback(async () => {
    if (!projectPath) return;
    setBusy(true);
    try {
      await invoke("setup_mcp_integration", { action: "claude_statusline", projectPath });
      reload();
    } finally {
      setBusy(false);
    }
  }, [projectPath, reload]);

  const dismiss = useCallback(() => {
    if (!projectPath) return;
    setDismissedPaths((prev) => new Set(prev).add(projectPath));
  }, [projectPath]);

  const needsSetup =
    (tools.claude && !tools.claudeMcpEnabled) ||
    (tools.codex && !tools.codexMcpEnabled) ||
    (tools.cursor && (!tools.cursorMcpEnabled || !tools.cursorApproved));
  const dismissed = projectPath ? dismissedPaths.has(projectPath) : false;

  return { tools, needsSetup, dismissed, busy, enable, enableHooks, enableStatusline, dismiss, reload };
}
