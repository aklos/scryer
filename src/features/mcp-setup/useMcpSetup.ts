import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Full per-project AI-tool state from `detect_ai_tools`: which CLIs are on
 *  PATH, and whether THIS project already has scryer wired into each one's
 *  config. The `*Enabled` / `*Approved` flags are always false when no project
 *  path is given (the PATH-only check used by the launch readout elsewhere). */
/** Where one tool's scryer hook install stands. `outdated` is an install from
 *  an earlier registration set, or one naming a binary that has since moved —
 *  offered as an update, never shown as missing. */
export type HookStatus = "none" | "outdated" | "current";

export type HookTool = "claude" | "codex" | "copilot";

/** Where each tool's hook registration lives, as the Settings rows and the
 *  update prompt name it. */
export const HOOK_TARGETS: Record<HookTool, string> = {
  claude: ".claude/settings.local.json",
  codex: ".codex/hooks.json",
  copilot: ".github/hooks/scryer.json",
};

export interface AiToolsState {
  claude: boolean;
  codex: boolean;
  copilot: boolean;
  claudeMcpEnabled: boolean;
  codexMcpEnabled: boolean;
  /** Copilot reads the same `.mcp.json` Claude Code does (and an optional
   *  committed `.github/mcp.json`), so this is usually true the moment
   *  `claudeMcpEnabled` is — no config file of its own. */
  copilotMcpEnabled: boolean;
  claudeApproved: boolean;
  /** Scryer's session hooks in this project's Claude Code settings. */
  claudeHooks: HookStatus;
  /** Scryer's session hooks in this project's `.codex/hooks.json`. */
  codexHooks: HookStatus;
  /** Scryer's session hooks in this project's `.github/hooks/scryer.json` —
   *  the only project-scoped location Copilot actually loads, and only once
   *  the folder is trusted. */
  copilotHooks: HookStatus;
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
  copilot: false,
  claudeMcpEnabled: false,
  codexMcpEnabled: false,
  copilotMcpEnabled: false,
  claudeApproved: false,
  claudeHooks: "none",
  codexHooks: "none",
  copilotHooks: "none",
  claudeStatuslineEnabled: false,
  claudeStatuslineForeign: false,
};

export interface McpSetup {
  tools: AiToolsState;
  /** A detected agent exists whose scryer MCP config this project is missing —
   *  the signal to offer setup. Tool auto-approve alone never nags (it rides
   *  along in `enable`, but its absence isn't worth a prompt). */
  needsSetup: boolean;
  /** Detected tools whose session hooks were installed from an earlier
   *  registration set — the signal to offer an update. Hooks are an opt-in, so
   *  only an install the user already made is ever offered again. */
  hooksOutdated: HookTool[];
  /** The user clicked "Not now" for this project this session. */
  dismissed: boolean;
  /** An enable write is in flight. */
  busy: boolean;
  /** Write every applicable config — `.mcp.json`, `.codex/config.toml`, and
   *  tool auto-approve in `.claude/settings.local.json` — then re-detect. */
  enable: () => Promise<void>;
  /** Explicit, separate opt-in: install scryer's session hooks for one tool —
   *  Claude Code (`.claude/settings.local.json`), Codex (`.codex/hooks.json`)
   *  or Copilot (`.github/hooks/scryer.json`). Never bundled into `enable` —
   *  the hooks change every session's behavior, so they
   *  are only written when the user asks for exactly that. */
  enableHooks: (tool: HookTool) => Promise<void>;
  /** Re-install every outdated hook registration in `hooksOutdated`. */
  updateHooks: () => Promise<void>;
  /** Its own opt-in, separate from the session hooks: register scryer's status
   *  one-liner as Claude Code's persistent statusLine. The only integration that
   *  keeps reporting while Scryer is closed (it reads the model off disk), so it
   *  is worth an install of its own rather than riding the app-gated hooks. */
  enableStatusline: () => Promise<void>;
  dismiss: () => void;
  /** Re-read detection from disk (e.g. after a config is written externally). */
  reload: () => void;
}

/** The detected tools whose session hooks are outdated. */
export function hooksToUpdate(tools: AiToolsState): HookTool[] {
  const status: Record<HookTool, HookStatus> = {
    claude: tools.claude ? tools.claudeHooks : "none",
    codex: tools.codex ? tools.codexHooks : "none",
    copilot: tools.copilot ? tools.copilotHooks : "none",
  };
  return (Object.keys(status) as HookTool[]).filter((t) => status[t] === "outdated");
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
      // One `.mcp.json` write serves Claude Code and Copilot both.
      if ((tools.claude && !tools.claudeMcpEnabled) || (tools.copilot && !tools.copilotMcpEnabled))
        actions.push("mcp");
      if (tools.codex && !tools.codexMcpEnabled) actions.push("mcp_codex");
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
    async (tool: HookTool) => {
      if (!projectPath) return;
      setBusy(true);
      try {
        const action = `${tool}_hooks`;
        await invoke("setup_mcp_integration", { action, projectPath });
        reload();
      } finally {
        setBusy(false);
      }
    },
    [projectPath, reload],
  );

  const hooksOutdated = hooksToUpdate(tools);

  const updateHooks = useCallback(async () => {
    if (!projectPath) return;
    setBusy(true);
    try {
      for (const tool of hooksToUpdate(tools)) {
        await invoke("setup_mcp_integration", { action: `${tool}_hooks`, projectPath });
      }
      reload();
    } finally {
      setBusy(false);
    }
  }, [projectPath, tools, reload]);

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
    (tools.copilot && !tools.copilotMcpEnabled);
  const dismissed = projectPath ? dismissedPaths.has(projectPath) : false;

  return {
    tools,
    needsSetup,
    hooksOutdated,
    dismissed,
    busy,
    enable,
    enableHooks,
    updateHooks,
    enableStatusline,
    dismiss,
    reload,
  };
}
