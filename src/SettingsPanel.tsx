import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Check, X } from "lucide-react";
import { Input, Select } from "./ui";
import { BTN, BTN_GO, BTN_ICON, EYEBROW, SegField } from "./pagekit";
import { useMcpSetup } from "./hooks/useMcpSetup";

type AgentPref = "auto" | "claudeCode" | "codex" | "cursor";

interface AgentSettings {
  model: string;
  effort: string;
}

export interface SubagentSettings {
  agent: AgentPref;
  claude: AgentSettings;
  codex: AgentSettings;
  cursor: AgentSettings;
  /** Confirm before a UI action launches an agent. "Don't ask again" clears it. */
  confirmLaunch: boolean;
}

export interface Detected {
  claude: boolean;
  codex: boolean;
  cursor: boolean;
}

const DEFAULT_AGENT: AgentSettings = { model: "", effort: "medium" };
export const SUBAGENT_DEFAULTS: SubagentSettings = {
  agent: "auto",
  claude: { ...DEFAULT_AGENT },
  codex: { ...DEFAULT_AGENT },
  cursor: { ...DEFAULT_AGENT },
  confirmLaunch: true,
};

/** Which agent a fill will actually use given the preference + what's installed,
 *  and the model + effort it runs with. Shared by the settings panel and the
 *  powerline so the launch readout and the editor can never disagree. `model`
 *  empty means the agent CLI's own default. */
export interface ResolvedLaunch {
  agent: "claudeCode" | "codex" | "cursor" | null;
  model: string;
  effort: string;
}

export const AGENT_LABEL: Record<"claudeCode" | "codex" | "cursor", string> = {
  claudeCode: "Claude Code",
  codex: "Codex",
  cursor: "Cursor",
};

function resolveAgent(
  pref: AgentPref,
  detected: Detected,
): ResolvedLaunch["agent"] {
  const pick = (
    primary: keyof Detected,
    fallbacks: (keyof Detected)[],
  ): ResolvedLaunch["agent"] => {
    if (detected[primary]) {
      return primary === "claude" ? "claudeCode" : primary;
    }
    for (const fb of fallbacks) {
      if (detected[fb]) {
        return fb === "claude" ? "claudeCode" : fb;
      }
    }
    return null;
  };

  switch (pref) {
    case "codex":
      return pick("codex", ["claude", "cursor"]);
    case "claudeCode":
      return pick("claude", ["codex", "cursor"]);
    case "cursor":
      return pick("cursor", ["claude", "codex"]);
    default:
      return pick("claude", ["codex", "cursor"]);
  }
}

export function resolveLaunch(settings: SubagentSettings, detected: Detected): ResolvedLaunch {
  const agent = resolveAgent(settings.agent, detected);
  const a =
    agent === "codex"
      ? settings.codex
      : agent === "cursor"
        ? settings.cursor
        : agent === "claudeCode"
          ? settings.claude
          : null;
  return { agent, model: a?.model ?? "", effort: a?.effort ?? "" };
}

// Effort levels are agent-specific (from each CLI's own option set).
const CLAUDE_EFFORT = ["low", "medium", "high", "xhigh", "max"];
const CODEX_EFFORT = ["minimal", "low", "medium", "high", "xhigh"];
const CURSOR_MODELS = ["composer-2.5-fast", "gpt-5.6-sol-medium", "claude-opus-5-thinking-high"];

// Curated models. Claude aliases auto-track the latest version; Codex uses
// explicit slugs. "Custom…" drops to a free-text field for anything else.
const CLAUDE_MODELS = ["opus", "sonnet", "haiku"];
const CODEX_MODELS = [
  "gpt-5.5",
  "gpt-5.4",
  "gpt-5.4-mini",
  "gpt-5.3-codex",
  "gpt-5.3-codex-spark",
];
const CUSTOM = "__custom__";

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export function SettingsPanel({
  onClose,
  projectPath,
}: {
  onClose: () => void;
  /** Current project, when one is open — enables the per-project session-hooks action. */
  projectPath?: string | null;
}) {
  const [settings, setSettings] = useState<SubagentSettings>(SUBAGENT_DEFAULTS);
  const [detected, setDetected] = useState<Detected>({ claude: false, codex: false, cursor: false });
  const [saving, setSaving] = useState(false);
  const mcpSetup = useMcpSetup(projectPath ?? null);

  useEffect(() => {
    invoke<SubagentSettings>("get_subagent_settings")
      .then((s) => setSettings({ ...SUBAGENT_DEFAULTS, ...s }))
      .catch(() => {});
    invoke<Detected>("detect_ai_tools", { projectPath: null })
      .then((d) =>
        setDetected({ claude: !!d.claude, codex: !!d.codex, cursor: !!d.cursor }),
      )
      .catch(() => {});
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const save = async () => {
    setSaving(true);
    try {
      await invoke("set_subagent_settings", { settings });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  // Which agent a fill will actually use, given preference + availability.
  const resolvedAgent = resolveLaunch(settings, detected).agent;

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/55 backdrop-blur-[3px]" onClick={onClose} />
      <div className="relative w-[440px] max-w-[90vw] rounded-lg border border-[var(--border-strong)] bg-[var(--surface)] shadow-2xl">
        <div className="flex items-center justify-between border-b border-[var(--border)] px-4 py-3">
          <span className="text-sm font-medium text-[var(--text)]">Subagent settings</span>
          <button
            type="button"
            onClick={onClose}
            className={BTN_ICON}
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="flex flex-col gap-4 px-4 py-4">
          <p className="text-xs text-[var(--text-muted)]">
            Controls which AI agent fills out the architecture model, and the model + reasoning
            effort it runs with. An empty model uses the agent CLI's own default.
          </p>

          <Field label="Detected agents">
            <div className="flex flex-wrap gap-4 text-xs">
              <AgentStatus name="Claude Code" available={detected.claude} />
              <AgentStatus name="Codex" available={detected.codex} />
              <AgentStatus name="Cursor" available={detected.cursor} />
            </div>
            {detected.cursor && mcpSetup.tools.cursor && !mcpSetup.tools.cursorAuthenticated && (
              <p className="mt-1 text-2xs text-amber-500">
                Cursor CLI found but not logged in — run <code className="font-mono">agent login</code> in a
                terminal.
              </p>
            )}
          </Field>

          <Field label="Agent">
            <SegField<AgentPref>
              options={[
                { value: "auto", label: "Auto" },
                { value: "claudeCode", label: "Claude Code" },
                { value: "codex", label: "Codex" },
                { value: "cursor", label: "Cursor" },
              ]}
              value={settings.agent}
              onChange={(agent) => setSettings((s) => ({ ...s, agent }))}
            />
            <p className="text-2xs text-[var(--text-muted)]">
              {resolvedAgent
                ? `Fills will use ${AGENT_LABEL[resolvedAgent]}.`
                : "No agent detected — install Claude Code, Codex, or Cursor CLI."}
            </p>
          </Field>

          <AgentSettingsGroup
            title="Claude Code"
            efforts={CLAUDE_EFFORT}
            models={CLAUDE_MODELS}
            value={settings.claude}
            onChange={(claude) => setSettings((s) => ({ ...s, claude }))}
          />

          <AgentSettingsGroup
            title="Codex"
            efforts={CODEX_EFFORT}
            models={CODEX_MODELS}
            value={settings.codex}
            onChange={(codex) => setSettings((s) => ({ ...s, codex }))}
          />

          <AgentSettingsGroup
            title="Cursor"
            efforts={[]}
            models={CURSOR_MODELS}
            value={settings.cursor}
            onChange={(cursor) => setSettings((s) => ({ ...s, cursor }))}
            hideEffort
          />

          {projectPath && (mcpSetup.tools.claude || mcpSetup.tools.codex) && (
            <Field label="Session hooks (this project)">
              <p className="text-2xs leading-relaxed text-[var(--text-muted)]">
                Let agent sessions see the model as they work: the status line on start, each
                file's claims and directives as they work in it, and a one-time close check for
                touched claims. Hooks are inert while Scryer is closed — installed per tool,
                active whenever Scryer has this project open.
              </p>
              {mcpSetup.tools.claude && (
                <HooksRow
                  name="Claude Code"
                  target=".claude/settings.local.json"
                  installed={mcpSetup.tools.claudeHooksEnabled}
                  busy={mcpSetup.busy}
                  onInstall={() => void mcpSetup.enableHooks("claude")}
                />
              )}
              {mcpSetup.tools.codex && (
                <HooksRow
                  name="Codex"
                  target=".codex/hooks.json"
                  installed={mcpSetup.tools.codexHooksEnabled}
                  busy={mcpSetup.busy}
                  onInstall={() => void mcpSetup.enableHooks("codex")}
                />
              )}
            </Field>
          )}

          {projectPath && mcpSetup.tools.claude && (
            <Field label="Status line (this project)">
              <p className="text-2xs leading-relaxed text-[var(--text-muted)]">
                Show the model's status — pending, drift, anchor health — in Claude Code's status
                line. Unlike the session hooks, it keeps reporting while Scryer is closed: the
                command reads the model straight off disk.
              </p>
              <StatuslineRow
                target=".claude/settings.local.json"
                installed={mcpSetup.tools.claudeStatuslineEnabled}
                foreign={mcpSetup.tools.claudeStatuslineForeign}
                busy={mcpSetup.busy}
                onInstall={() => void mcpSetup.enableStatusline()}
              />
            </Field>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" onClick={onClose} className={BTN}>
            Cancel
          </button>
          <button
            type="button"
            onClick={save}
            disabled={saving}
            className={`${BTN_GO} disabled:cursor-default disabled:opacity-50`}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

/** A labelled field — the label rendered as the wiki design's uppercase eyebrow
 *  (matching the section headers on the node pages), then its control below. */
function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className={EYEBROW}>{label}</span>
      {children}
    </div>
  );
}

/** One tool's hook-install state: installed check, or the target file + button. */
function HooksRow({
  name,
  target,
  installed,
  busy,
  onInstall,
}: {
  name: string;
  target: string;
  installed: boolean;
  busy: boolean;
  onInstall: () => void;
}) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-xs text-[var(--text)]">{name}</span>
      {installed ? (
        <span className="flex items-center gap-1 text-xs text-emerald-500">
          <Check className="h-3 w-3" /> Installed
        </span>
      ) : (
        <span className="flex items-center gap-2">
          <span className="font-mono text-2xs text-[var(--text-muted)]">{target}</span>
          <button type="button" className={BTN} disabled={busy} onClick={onInstall}>
            {busy ? "Installing…" : "Install"}
          </button>
        </span>
      )}
    </div>
  );
}

/** The statusLine install state. Like a HooksRow, but statusLine is a single
 *  slot (whole-line replacement), so a foreign line is never clobbered — that
 *  case shows a note in place of the Install button. */
function StatuslineRow({
  target,
  installed,
  foreign,
  busy,
  onInstall,
}: {
  target: string;
  installed: boolean;
  foreign: boolean;
  busy: boolean;
  onInstall: () => void;
}) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-xs text-[var(--text)]">Claude Code</span>
      {installed ? (
        <span className="flex items-center gap-1 text-xs text-emerald-500">
          <Check className="h-3 w-3" /> Installed
        </span>
      ) : foreign ? (
        <span className="text-2xs text-[var(--text-muted)]">
          A status line is already configured — left untouched.
        </span>
      ) : (
        <span className="flex items-center gap-2">
          <span className="font-mono text-2xs text-[var(--text-muted)]">{target}</span>
          <button type="button" className={BTN} disabled={busy} onClick={onInstall}>
            {busy ? "Installing…" : "Install"}
          </button>
        </span>
      )}
    </div>
  );
}

function AgentStatus({ name, available }: { name: string; available: boolean }) {
  return (
    <span className={`flex items-center gap-1 ${available ? "text-emerald-500" : "text-[var(--text-ghost)]"}`}>
      {available ? <Check className="h-3 w-3" /> : <X className="h-3 w-3" />}
      {name}
    </span>
  );
}

/** A titled block of one agent's own settings: reasoning effort + model. */
function AgentSettingsGroup({
  title,
  efforts,
  models,
  value,
  onChange,
  hideEffort,
}: {
  title: string;
  efforts: string[];
  models: string[];
  value: AgentSettings;
  onChange: (next: AgentSettings) => void;
  hideEffort?: boolean;
}) {
  return (
    <div className="rounded-md border border-[var(--border)] p-3">
      <h3 className={`mb-2.5 border-b border-[var(--border)] pb-[5px] ${EYEBROW}`}>
        {title}
      </h3>
      <div className="flex flex-col gap-3">
        {!hideEffort && (
          <Field label="Reasoning effort">
            <SegField
              options={efforts.map((e) => ({ value: e, label: cap(e) }))}
              value={value.effort}
              onChange={(effort) => onChange({ ...value, effort })}
            />
          </Field>
        )}
        <Field label="Model">
          <ModelPicker
            aliases={models}
            value={value.model}
            onChange={(model) => onChange({ ...value, model })}
          />
        </Field>
      </div>
    </div>
  );
}

/** Dropdown of curated aliases ("Default" = empty) plus a "Custom…" escape that
 *  reveals a free-text field for full model names. */
function ModelPicker({
  aliases,
  value,
  onChange,
}: {
  aliases: string[];
  value: string;
  onChange: (value: string) => void;
}) {
  // Custom mode when the stored value is non-empty and not a known alias.
  const isKnown = value === "" || aliases.includes(value);
  const [custom, setCustom] = useState(!isKnown);

  const selectValue = custom ? CUSTOM : value;
  const options = [
    { value: "", label: "Default" },
    ...aliases.map((a) => ({ value: a, label: a })),
    { value: CUSTOM, label: "Custom…" },
  ];

  return (
    <div className="flex flex-col gap-1.5">
      <Select
        variant="bordered"
        options={options}
        value={selectValue}
        onChange={(v) => {
          if (v === CUSTOM) {
            setCustom(true);
          } else {
            setCustom(false);
            onChange(v);
          }
        }}
      />
      {custom && (
        <Input
          variant="bordered"
          value={value}
          placeholder="Full model name (e.g. claude-opus-4-7)"
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </div>
  );
}
