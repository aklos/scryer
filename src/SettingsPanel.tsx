import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Input, Select } from "./ui";
import {
  type ThemeConfig,
  type ColorMode,
  THEME_ROLES,
  PALETTES,
  SHADES,
  DEFAULT_THEME,
  applyTheme,
  saveTheme,
  isDefaultTheme,
} from "./theme";
import { ChevronUp, ChevronDown, Sun, Moon, Monitor } from "lucide-react";

// ── Constants ──

const PROVIDERS = [
  { value: "openai", label: "OpenAI", needsKey: true },
  { value: "anthropic", label: "Anthropic", needsKey: true },
  { value: "google", label: "Google", needsKey: true },
  { value: "groq", label: "Groq", needsKey: true },
  { value: "mistral", label: "Mistral", needsKey: true },
  { value: "deepseek", label: "DeepSeek", needsKey: true },
  { value: "ollama", label: "Ollama (local)", needsKey: false },
];

type AiSettingsResponse = {
  provider: string;
  model: string;
  hasKey: boolean;
  configured: boolean;
};

type SettingsTab = "ai" | "theme";

// ── Theme sub-components ──

function PalettePreview({ name, offset = 0 }: { name: string; offset?: number }) {
  const p = PALETTES[name];
  if (!p) return null;
  const previewShades = ["300", "400", "500", "600", "700"] as const;
  return (
    <div className="flex gap-px">
      {previewShades.map((shade) => {
        const idx = SHADES.indexOf(shade);
        const shifted = SHADES[Math.max(0, Math.min(SHADES.length - 1, idx + offset))];
        return (
          <div
            key={shade}
            className="w-3 h-3 rounded-sm first:rounded-l last:rounded-r"
            style={{ backgroundColor: p[shifted] }}
          />
        );
      })}
    </div>
  );
}

function ShadeOffset({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  return (
    <div className="flex flex-col -gap-0.5">
      <button
        type="button"
        disabled={value >= 3}
        className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 disabled:opacity-20 cursor-pointer disabled:cursor-default"
        onClick={() => onChange(value + 1)}
      >
        <ChevronUp size={12} />
      </button>
      <button
        type="button"
        disabled={value <= -3}
        className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 disabled:opacity-20 cursor-pointer disabled:cursor-default"
        onClick={() => onChange(value - 1)}
      >
        <ChevronDown size={12} />
      </button>
    </div>
  );
}

function ShadeStrip({
  palette,
  value,
  hasWhite,
  reverse,
  onChange,
}: {
  palette: string;
  value: number;
  hasWhite?: boolean;
  reverse?: boolean;
  onChange: (v: number) => void;
}) {
  const p = PALETTES[palette];
  if (!p) return null;
  const indices = reverse ? [5, 6, 7, 8, 9] : [0, 1, 2, 3, 4];
  return (
    <div className="flex gap-px items-center">
      {hasWhite && !reverse && (
        <button
          type="button"
          className={`w-5 h-5 rounded-sm border cursor-pointer ${
            value === -1
              ? "border-zinc-900 dark:border-zinc-100 ring-1 ring-zinc-900 dark:ring-zinc-100"
              : "border-zinc-300 dark:border-zinc-600"
          }`}
          style={{ backgroundColor: "#ffffff" }}
          title="White"
          onClick={() => onChange(-1)}
        />
      )}
      {indices.map((idx) => (
        <button
          key={idx}
          type="button"
          className={`w-5 h-5 rounded-sm border cursor-pointer ${
            value === idx
              ? "border-zinc-900 dark:border-zinc-100 ring-1 ring-zinc-900 dark:ring-zinc-100"
              : "border-zinc-300 dark:border-zinc-600"
          }`}
          style={{ backgroundColor: p[SHADES[idx]] }}
          title={SHADES[idx]}
          onClick={() => onChange(idx)}
        />
      ))}
    </div>
  );
}

// ── Tab content components ──

function AiReviewTab({
  provider, setProvider, apiKey, setApiKey, model, setModel,
  hasExistingKey, saving, saved, error, onSave,
}: {
  provider: string; setProvider: (v: string) => void;
  apiKey: string; setApiKey: (v: string) => void;
  model: string; setModel: (v: string) => void;
  hasExistingKey: boolean; saving: boolean; saved: boolean; error: string | null;
  onSave: () => void;
}) {
  const currentProvider = PROVIDERS.find((p) => p.value === provider);
  const [modelList, setModelList] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);

  const loadModels = useCallback((prov: string, key?: string) => {
    const p = PROVIDERS.find((p) => p.value === prov);
    if (!p) return;
    setModelsLoading(true);
    setModelsError(null);
    invoke<string[]>("fetch_models", { provider: prov, apiKey: key || null })
      .then((models) => {
        setModelList(models);
      })
      .catch((e) => {
        setModelsError(String(e));
        setModelList([]);
      })
      .finally(() => setModelsLoading(false));
  }, []);

  // Fetch after settings are loaded (hasExistingKey signals that get_ai_settings resolved)
  useEffect(() => {
    if (provider && (hasExistingKey || !currentProvider?.needsKey)) {
      loadModels(provider);
    }
  }, [hasExistingKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const modelOptions = [
    ...modelList.map((m) => ({ value: m, label: m })),
    // Keep current model in list even if not returned by API
    ...(!modelList.includes(model) && model ? [{ value: model, label: model }] : []),
  ];

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-zinc-500 dark:text-zinc-400">
        AI Review sends your diagram to an LLM that checks for C4 modeling issues — missing relationships, naming problems, structural anti-patterns. Configure a provider below to enable the <span className="text-violet-500 dark:text-violet-400">Review</span> button on the canvas.
      </p>
      <div>
        <label className="text-[10px] uppercase tracking-wider text-zinc-400 dark:text-zinc-500">Provider</label>
        <Select
          variant="bordered"
          options={PROVIDERS.map((p) => ({ value: p.value, label: p.label }))}
          value={provider}
          onChange={(v) => {
            setProvider(v);
            setModel("");
            setApiKey("");
            setModelList([]);
            loadModels(v);
          }}
        />
      </div>
      {currentProvider?.needsKey && (
        <div>
          <label className="text-[10px] uppercase tracking-wider text-zinc-400 dark:text-zinc-500">API Key</label>
          <Input
            type="password"
            value={apiKey}
            placeholder={hasExistingKey ? "(saved — leave blank to keep)" : "sk-..."}
            onChange={(e) => setApiKey(e.target.value)}
            onBlur={() => { if (apiKey || hasExistingKey) loadModels(provider, apiKey); }}
          />
        </div>
      )}
      <div>
        <label className="text-[10px] uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
          Model
          {modelsLoading && <span className="ml-1 text-zinc-400 dark:text-zinc-500 normal-case tracking-normal">(loading...)</span>}
        </label>
        {modelOptions.length > 0 ? (
          <Select
            variant="bordered"
            searchable
            options={modelOptions}
            value={model}
            onChange={setModel}
          />
        ) : (
          <Input
            value={model}
            placeholder={modelsError ? "Could not fetch models — type manually" : "Enter model name"}
            onChange={(e) => setModel(e.target.value)}
          />
        )}
        {modelsError && <p className="text-[10px] text-amber-500 mt-0.5">{modelsError}</p>}
      </div>
      {error && <p className="text-xs text-red-500">{error}</p>}
      <Button variant={saved ? "ghost" : "primary"} size="md" disabled={saving || !model} onClick={onSave}>
        {saving ? "Saving..." : saved ? "Saved" : "Save"}
      </Button>
    </div>
  );
}

function ThemeTab({
  theme,
  onThemeChange,
}: {
  theme: ThemeConfig;
  onThemeChange: (theme: ThemeConfig) => void;
}) {
  const [localTheme, setLocalTheme] = useState(theme);

  const commit = useCallback(
    (next: ThemeConfig) => {
      setLocalTheme(next);
      applyTheme(next);
      saveTheme(next);
      onThemeChange(next);
    },
    [onThemeChange],
  );

  return (
    <div className="flex flex-col gap-4">
      {/* Color mode */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">Appearance</div>
        <div className="flex gap-1">
          {([
            { mode: "light" as ColorMode, icon: Sun, label: "Light" },
            { mode: "dark" as ColorMode, icon: Moon, label: "Dark" },
            { mode: "system" as ColorMode, icon: Monitor, label: "System" },
          ]).map(({ mode, icon: Icon, label }) => (
            <button
              key={mode}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium cursor-pointer transition-colors ${
                localTheme.colorMode === mode
                  ? "bg-zinc-200 text-zinc-800 dark:bg-zinc-600 dark:text-zinc-100"
                  : "text-zinc-500 hover:text-zinc-700 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-700"
              }`}
              onClick={() => commit({ ...localTheme, colorMode: mode })}
            >
              <Icon size={14} />
              {label}
            </button>
          ))}
        </div>
      </div>

      {/* Light mode backgrounds */}
      <div className="flex flex-col gap-3">
        <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">Light mode</div>
        <div className="flex items-center gap-3">
          <div className="flex-1 min-w-0"><div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Canvas</div></div>
          <ShadeStrip palette={localTheme.zinc} value={localTheme.canvasLight} onChange={(v) => commit({ ...localTheme, canvasLight: v })} />
        </div>
        <div className="flex items-center gap-3">
          <div className="flex-1 min-w-0"><div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Nodes</div></div>
          <ShadeStrip palette={localTheme.zinc} value={localTheme.nodeLight} hasWhite onChange={(v) => commit({ ...localTheme, nodeLight: v })} />
        </div>
      </div>

      {/* Dark mode backgrounds */}
      <div className="flex flex-col gap-3">
        <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">Dark mode</div>
        <div className="flex items-center gap-3">
          <div className="flex-1 min-w-0"><div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Canvas</div></div>
          <ShadeStrip palette={localTheme.zinc} value={localTheme.canvasDark} reverse onChange={(v) => commit({ ...localTheme, canvasDark: v })} />
        </div>
        <div className="flex items-center gap-3">
          <div className="flex-1 min-w-0"><div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Nodes</div></div>
          <ShadeStrip palette={localTheme.zinc} value={localTheme.nodeDark} reverse onChange={(v) => commit({ ...localTheme, nodeDark: v })} />
        </div>
      </div>

      {/* Palette roles */}
      <div className="flex flex-col gap-3">
        <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">Colors</div>
        {THEME_ROLES.map((role) => {
          const offset = localTheme.offsets[role.key] ?? 0;
          return (
            <div key={role.key} className="flex items-center gap-3">
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">{role.label}</div>
                <div className="text-[10px] text-zinc-400 dark:text-zinc-500 truncate">{role.description}</div>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <PalettePreview name={localTheme[role.key]} offset={offset} />
                <Select
                  options={role.palettes.map((p) => ({ value: p, label: p }))}
                  value={localTheme[role.key]}
                  onChange={(v) => commit({ ...localTheme, [role.key]: v })}
                />
                <ShadeOffset value={offset} onChange={(v) => {
                  const offsets = { ...localTheme.offsets, [role.key]: v };
                  if (v === 0) delete offsets[role.key];
                  commit({ ...localTheme, offsets });
                }} />
              </div>
            </div>
          );
        })}
      </div>

      {/* Reset */}
      <Button
        variant="ghost"
        size="sm"
        disabled={isDefaultTheme(localTheme)}
        onClick={() => commit({ ...DEFAULT_THEME, offsets: {} })}
      >
        Reset to defaults
      </Button>
    </div>
  );
}

// ── Main panel ──

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "ai", label: "AI Review" },
  { id: "theme", label: "Theme" },
];

export function SettingsPanel({
  onClose,
  onSaved,
  theme,
  onThemeChange,
  initialTab,
}: {
  onClose: () => void;
  onSaved: (configured: boolean) => void;
  theme: ThemeConfig;
  onThemeChange: (theme: ThemeConfig) => void;
  initialTab?: SettingsTab;
}) {
  const [tab, setTab] = useState<SettingsTab>(initialTab ?? "ai");
  const [provider, setProvider] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [hasExistingKey, setHasExistingKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    invoke<AiSettingsResponse>("get_ai_settings").then((s) => {
      setProvider(s.provider || "openai");
      setModel(s.model || "gpt-5-nano");
      setHasExistingKey(s.hasKey);
    }).catch(() => {});
  }, []);

  const handleSave = useCallback(async () => {
    const currentProvider = PROVIDERS.find((p) => p.value === provider);
    if (currentProvider?.needsKey && !apiKey && !hasExistingKey) {
      setError("API key is required");
      return;
    }
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      await invoke("save_ai_settings", { provider, apiKey, model });
      const s = await invoke<AiSettingsResponse>("get_ai_settings");
      onSaved(s.configured);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, [provider, apiKey, model, hasExistingKey, onSaved]);

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/20 backdrop-blur-[2px]">
      <div className="w-[420px] max-h-[80vh] rounded-lg border border-zinc-200 bg-white shadow-lg dark:border-zinc-700 dark:bg-zinc-800 flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-zinc-200 px-4 py-3 dark:border-zinc-700">
          <h2 className="text-sm font-medium text-zinc-800 dark:text-zinc-100">Settings</h2>
          <button
            type="button"
            className="text-zinc-400 hover:text-zinc-600 cursor-pointer dark:text-zinc-500 dark:hover:text-zinc-300"
            onClick={onClose}
          >
            &times;
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-zinc-200 dark:border-zinc-700 px-4">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`px-3 py-2 text-xs font-medium cursor-pointer transition-colors border-b-2 -mb-px ${
                tab === t.id
                  ? "border-blue-500 text-blue-600 dark:text-blue-400"
                  : "border-transparent text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200"
              }`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* Content */}
        {tab === "ai" && (
          <div className="p-4">
            <AiReviewTab
              provider={provider} setProvider={setProvider}
              apiKey={apiKey} setApiKey={setApiKey}
              model={model} setModel={setModel}
              hasExistingKey={hasExistingKey}
              saving={saving} saved={saved} error={error}
              onSave={handleSave}
            />
          </div>
        )}
        {tab === "theme" && (
          <div className="flex-1 overflow-y-auto p-4">
            <ThemeTab theme={theme} onThemeChange={onThemeChange} />
          </div>
        )}
      </div>
    </div>
  );
}
