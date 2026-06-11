import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Check, X } from "lucide-react";
import { Button, Field, Input, SegmentedControl, Select } from "./ui";

type AgentPref = "auto" | "claudeCode" | "codex";

interface AgentSettings {
  model: string;
  effort: string;
}

export interface SubagentSettings {
  agent: AgentPref;
  claude: AgentSettings;
  codex: AgentSettings;
}

interface Detected {
  claude: boolean;
  codex: boolean;
}

const DEFAULT_AGENT: AgentSettings = { model: "", effort: "medium" };
const DEFAULTS: SubagentSettings = {
  agent: "auto",
  claude: { ...DEFAULT_AGENT },
  codex: { ...DEFAULT_AGENT },
};

// Effort levels are agent-specific (from each CLI's own option set).
const CLAUDE_EFFORT = ["low", "medium", "high", "xhigh", "max"];
const CODEX_EFFORT = ["minimal", "low", "medium", "high", "xhigh"];

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

export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [settings, setSettings] = useState<SubagentSettings>(DEFAULTS);
  const [detected, setDetected] = useState<Detected>({ claude: false, codex: false });
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    invoke<SubagentSettings>("get_subagent_settings")
      .then((s) => setSettings({ ...DEFAULTS, ...s }))
      .catch(() => {});
    invoke<Detected>("detect_ai_tools", { projectPath: null })
      .then((d) => setDetected({ claude: !!d.claude, codex: !!d.codex }))
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
  const resolvedAgent: "claudeCode" | "codex" | null = (() => {
    const c = detected.claude;
    const x = detected.codex;
    if (settings.agent === "codex") return x ? "codex" : c ? "claudeCode" : null;
    if (settings.agent === "claudeCode") return c ? "claudeCode" : x ? "codex" : null;
    return c ? "claudeCode" : x ? "codex" : null;
  })();

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/55 backdrop-blur-[3px]" onClick={onClose} />
      <div className="relative w-[440px] max-w-[90vw] rounded-lg border border-[var(--border-strong)] bg-[var(--surface)] shadow-2xl">
        <div className="flex items-center justify-between border-b border-[var(--border)] px-4 py-3">
          <span className="text-sm font-medium text-[var(--text)]">Subagent settings</span>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-0.5 text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
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
            <div className="flex gap-4 text-xs">
              <AgentStatus name="Claude Code" available={detected.claude} />
              <AgentStatus name="Codex" available={detected.codex} />
            </div>
          </Field>

          <Field label="Agent">
            <SegmentedControl
              options={[
                { value: "auto", label: "Auto" },
                { value: "claudeCode", label: "Claude Code" },
                { value: "codex", label: "Codex" },
              ]}
              value={settings.agent}
              onChange={(agent) => setSettings((s) => ({ ...s, agent: agent as AgentPref }))}
            />
            <p className="mt-1 text-2xs text-[var(--text-muted)]">
              {resolvedAgent
                ? `Fills will use ${resolvedAgent === "claudeCode" ? "Claude Code" : "Codex"}.`
                : "No agent detected — install Claude Code or Codex."}
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
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" onClick={save} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>
    </div>,
    document.body,
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
}: {
  title: string;
  efforts: string[];
  models: string[];
  value: AgentSettings;
  onChange: (next: AgentSettings) => void;
}) {
  return (
    <div className="rounded-md border border-[var(--border)] p-3">
      <div className="mb-2 text-xs font-medium text-[var(--text-secondary)]">{title}</div>
      <div className="flex flex-col gap-3">
        <Field label="Reasoning effort">
          <SegmentedControl
            options={efforts.map((e) => ({ value: e, label: cap(e) }))}
            value={value.effort}
            onChange={(effort) => onChange({ ...value, effort })}
          />
        </Field>
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
