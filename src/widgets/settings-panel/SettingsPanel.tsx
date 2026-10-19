/**
 * The settings panel: the project's one agent-integration switch, and which
 * coding agent the app launches for preview fixtures, with its model and
 * reasoning effort.
 */

import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Check, X } from "lucide-react";
import { Input, Select } from "../../shared/ui";
import { BTN, BTN_GO, BTN_ICON, EYEBROW, SegField } from "../../shared/ui/pagekit";
import { AgentIntegrationSwitch } from "../../features/agent-integration/AgentIntegrationSwitch";
import {
  AGENT_KEY,
  AGENT_LABEL,
  AGENTS,
  type AgentPref,
  type AgentSettings,
  type Detected,
  type LaunchAgent,
  resolveLaunch,
  SUBAGENT_DEFAULTS,
  type SubagentSettings,
} from "../../features/agent-launch/launchSettings";

// Effort levels are agent-specific (from each CLI's own option set).
const CLAUDE_EFFORT = ["low", "medium", "high", "xhigh", "max"];
const CODEX_EFFORT = ["minimal", "low", "medium", "high", "xhigh"];
const COPILOT_EFFORT = ["none", "low", "medium", "high", "xhigh", "max"];

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
// Copilot fronts several providers on one subscription — the reason to want it
// — so its shortlist spans them rather than favouring one. "auto" is Copilot's
// own per-task pick, distinct from "Default" (whatever the CLI is set to).
const COPILOT_MODELS = [
  "auto",
  "claude-opus-4.8",
  "claude-sonnet-4.6",
  "claude-haiku-4.5",
  "gpt-5.5",
  "gpt-5.3-codex",
  "gemini-3.1-pro-preview",
];
const CUSTOM = "__custom__";

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export function SettingsPanel({
  onClose,
  projectPath,
}: {
  onClose: () => void;
  /** Current project, when one is open — shows its agent-integration switch. */
  projectPath?: string | null;
}) {
  const [settings, setSettings] = useState<SubagentSettings>(SUBAGENT_DEFAULTS);
  const [detected, setDetected] = useState<Detected>({
    claude: false,
    codex: false,
    copilot: false,
  });
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    invoke<SubagentSettings>("get_subagent_settings")
      .then((s) => setSettings({ ...SUBAGENT_DEFAULTS, ...s }))
      .catch(() => {});
    invoke<Detected>("detect_ai_tools")
      .then((d) => setDetected({ claude: !!d.claude, codex: !!d.codex, copilot: !!d.copilot }))
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
            Which AI agent the app launches to write preview fixtures, and the model + reasoning
            effort it runs with. An empty model uses the agent CLI's own default.
          </p>

          <Field label="Detected agents">
            <div className="flex flex-wrap gap-4 text-xs">
              {AGENTS.map((a) => (
                <AgentStatus key={a} name={AGENT_LABEL[a]} available={detected[AGENT_KEY[a]]} />
              ))}
            </div>
          </Field>

          <Field label="Agent">
            <SegField<AgentPref>
              options={[
                { value: "auto", label: "Auto" },
                ...AGENTS.map((a) => ({ value: a, label: AGENT_LABEL[a] })),
              ]}
              value={settings.agent}
              onChange={(agent) => setSettings((s) => ({ ...s, agent }))}
            />
            <p className="text-xs text-[var(--text-muted)]">
              {resolvedAgent
                ? `Fills will use ${AGENT_LABEL[resolvedAgent]}.`
                : "No agent detected — install Claude Code, Codex or Copilot CLI."}
            </p>
          </Field>

          {/* Only the agent that will actually run gets its model + effort
              group — the other two are noise until they are picked. "Auto"
              shows the resolved one and says so. */}
          {(() => {
            const shown: LaunchAgent | null =
              settings.agent === "auto" ? resolvedAgent : settings.agent;
            if (!shown) return null;
            const note = settings.agent === "auto" ? " (auto-selected)" : "";
            switch (shown) {
              case "claudeCode":
                return (
                  <AgentSettingsGroup
                    key="claudeCode"
                    title={`Claude Code${note}`}
                    efforts={CLAUDE_EFFORT}
                    models={CLAUDE_MODELS}
                    value={settings.claude}
                    onChange={(claude) => setSettings((s) => ({ ...s, claude }))}
                  />
                );
              case "codex":
                return (
                  <AgentSettingsGroup
                    key="codex"
                    title={`Codex${note}`}
                    efforts={CODEX_EFFORT}
                    models={CODEX_MODELS}
                    value={settings.codex}
                    onChange={(codex) => setSettings((s) => ({ ...s, codex }))}
                  />
                );
              case "copilot":
                return (
                  <AgentSettingsGroup
                    key="copilot"
                    title={`Copilot CLI${note}`}
                    efforts={COPILOT_EFFORT}
                    models={COPILOT_MODELS}
                    customNote="Copilot ignores a model name it doesn't recognise instead of reporting it, so a typo here runs on its default model rather than failing. Check the spelling against `copilot help config`."
                    value={settings.copilot}
                    onChange={(copilot) => setSettings((s) => ({ ...s, copilot }))}
                  />
                );
            }
          })()}

          {projectPath && (
            <Field label="This project">
              <AgentIntegrationSwitch projectPath={projectPath} />
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
  customNote,
  value,
  onChange,
}: {
  title: string;
  efforts: string[];
  models: string[];
  /** Shown only when the free-text model field is open — for an agent whose CLI
   *  won't tell the user when the name they typed doesn't land. */
  customNote?: string;
  value: AgentSettings;
  onChange: (next: AgentSettings) => void;
}) {
  return (
    <div className="rounded-md border border-[var(--border)] p-3">
      <h3 className={`mb-2.5 border-b border-[var(--border)] pb-[5px] ${EYEBROW}`}>
        {title}
      </h3>
      <div className="flex flex-col gap-3">
        <Field label="Reasoning effort">
          <SegField
            options={efforts.map((e) => ({ value: e, label: cap(e) }))}
            value={value.effort}
            onChange={(effort) => onChange({ ...value, effort })}
          />
        </Field>
        <Field label="Model">
          <ModelPicker
            aliases={models}
            customNote={customNote}
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
  customNote,
  value,
  onChange,
}: {
  aliases: string[];
  customNote?: string;
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
        <>
          <Input
            variant="bordered"
            value={value}
            placeholder="Full model name (e.g. claude-opus-4-7)"
            onChange={(e) => onChange(e.target.value)}
          />
          {customNote && (
            <p className="text-xs leading-relaxed text-[var(--text-muted)]">{customNote}</p>
          )}
        </>
      )}
    </div>
  );
}
