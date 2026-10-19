/**
 * Launch settings — which coding agent a fill runs with, its model and
 * reasoning effort, and how a stored preference resolves against the agents
 * actually installed.
 */

/** The agents a fill can run with, in the order they're fallen back to when the
 *  preferred one isn't installed. Mirrors `detect_available_agent_pref`, so the
 *  settings panel's readout and the agent that actually launches can't disagree. */
export const AGENTS = ["claudeCode", "codex", "copilot"] as const;
export type LaunchAgent = (typeof AGENTS)[number];
export type AgentPref = "auto" | LaunchAgent;

export interface AgentSettings {
  model: string;
  effort: string;
}

export interface SubagentSettings {
  agent: AgentPref;
  claude: AgentSettings;
  codex: AgentSettings;
  copilot: AgentSettings;
  /** Confirm before a UI action launches an agent. "Don't ask again" clears it. */
  confirmLaunch: boolean;
}

export interface Detected {
  claude: boolean;
  codex: boolean;
  copilot: boolean;
}

/** Each agent's key in `Detected` and in the settings blocks — the same word in
 *  both, so one map serves the availability check and the model/effort lookup. */
export const AGENT_KEY: Record<LaunchAgent, "claude" | "codex" | "copilot"> = {
  claudeCode: "claude",
  codex: "codex",
  copilot: "copilot",
};

const DEFAULT_AGENT: AgentSettings = { model: "", effort: "medium" };
export const SUBAGENT_DEFAULTS: SubagentSettings = {
  agent: "auto",
  claude: { ...DEFAULT_AGENT },
  codex: { ...DEFAULT_AGENT },
  copilot: { ...DEFAULT_AGENT },
  confirmLaunch: true,
};

/** Which agent a fill will actually use given the preference + what's installed,
 *  and the model + effort it runs with. Shared by the settings panel and the
 *  powerline so the launch readout and the editor can never disagree. `model`
 *  empty means the agent CLI's own default. */
export interface ResolvedLaunch {
  agent: LaunchAgent | null;
  model: string;
  effort: string;
}

export const AGENT_LABEL: Record<LaunchAgent, string> = {
  claudeCode: "Claude Code",
  codex: "Codex",
  copilot: "Copilot CLI",
};

export function resolveLaunch(settings: SubagentSettings, detected: Detected): ResolvedLaunch {
  const installed = (a: LaunchAgent) => detected[AGENT_KEY[a]];
  // Preference first, then the standing order — so an uninstalled preference
  // degrades to a working agent rather than to nothing.
  const order: LaunchAgent[] =
    settings.agent === "auto" ? [...AGENTS] : [settings.agent, ...AGENTS];
  const agent = order.find(installed) ?? null;
  const a = agent ? settings[AGENT_KEY[agent]] : null;
  return { agent, model: a?.model ?? "", effort: a?.effort ?? "" };
}
