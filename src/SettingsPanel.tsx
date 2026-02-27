import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Input, Select } from "./ui";

const PROVIDERS = [
  { value: "openai", label: "OpenAI", defaultModel: "gpt-5-nano", needsKey: true },
  { value: "anthropic", label: "Anthropic", defaultModel: "claude-haiku-4-5-20251001", needsKey: true },
  { value: "google", label: "Google", defaultModel: "gemini-3-flash-preview", needsKey: true },
  { value: "groq", label: "Groq", defaultModel: "qwen-qwq-32b", needsKey: true },
  { value: "mistral", label: "Mistral", defaultModel: "mistral-small-latest", needsKey: true },
  { value: "deepseek", label: "DeepSeek", defaultModel: "deepseek-chat", needsKey: true },
  { value: "ollama", label: "Ollama (local)", defaultModel: "llama3.2:3b", needsKey: false },
] as const;

type AiSettingsResponse = {
  provider: string;
  model: string;
  hasKey: boolean;
  configured: boolean;
};

export function SettingsPanel({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: (configured: boolean) => void;
}) {
  const [provider, setProvider] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [hasExistingKey, setHasExistingKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<AiSettingsResponse>("get_ai_settings").then((s) => {
      setProvider(s.provider || "openai");
      setModel(s.model || "gpt-5-nano");
      setHasExistingKey(s.hasKey);
    }).catch(() => {});
  }, []);

  const currentProvider = PROVIDERS.find((p) => p.value === provider);

  const handleProviderChange = useCallback((newProvider: string) => {
    setProvider(newProvider);
    const p = PROVIDERS.find((p) => p.value === newProvider);
    if (p) setModel(p.defaultModel);
    setApiKey("");
    setError(null);
  }, []);

  const handleSave = useCallback(async () => {
    if (currentProvider?.needsKey && !apiKey && !hasExistingKey) {
      setError("API key is required");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await invoke("save_ai_settings", { provider, apiKey, model });
      const s = await invoke<AiSettingsResponse>("get_ai_settings");
      onSaved(s.configured);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, [provider, apiKey, model, currentProvider, hasExistingKey, onSaved, onClose]);

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/20 backdrop-blur-[2px]">
      <div className="w-80 rounded-lg border border-zinc-200 bg-white shadow-lg dark:border-zinc-700 dark:bg-zinc-800">
        <div className="flex items-center justify-between border-b border-zinc-200 px-4 py-3 dark:border-zinc-700">
          <h2 className="text-sm font-medium text-zinc-800 dark:text-zinc-100">AI Settings</h2>
          <button
            type="button"
            className="text-zinc-400 hover:text-zinc-600 cursor-pointer dark:text-zinc-500 dark:hover:text-zinc-300"
            onClick={onClose}
          >
            &times;
          </button>
        </div>
        <div className="p-4 flex flex-col gap-3">
          {/* Provider */}
          <div>
            <label className="text-[10px] uppercase tracking-wider text-zinc-400 dark:text-zinc-500">Provider</label>
            <Select
              options={PROVIDERS.map((p) => ({ value: p.value, label: p.label }))}
              value={provider}
              onChange={handleProviderChange}
            />
          </div>

          {/* API Key */}
          {currentProvider?.needsKey && (
            <div>
              <label className="text-[10px] uppercase tracking-wider text-zinc-400 dark:text-zinc-500">API Key</label>
              <Input
                type="password"
                value={apiKey}
                placeholder={hasExistingKey ? "(saved — leave blank to keep)" : "sk-..."}
                onChange={(e) => setApiKey(e.target.value)}
              />
            </div>
          )}

          {/* Model */}
          <div>
            <label className="text-[10px] uppercase tracking-wider text-zinc-400 dark:text-zinc-500">Model</label>
            <Input
              value={model}
              placeholder="model name"
              onChange={(e) => setModel(e.target.value)}
            />
          </div>

          {error && (
            <p className="text-xs text-red-500">{error}</p>
          )}

          {/* Actions */}
          <div className="flex gap-2 pt-1">
            <Button variant="secondary" size="md" className="flex-1" onClick={onClose}>
              Cancel
            </Button>
            <Button variant="primary" size="md" className="flex-1" disabled={saving || !model} onClick={handleSave}>
              {saving ? "Saving..." : "Save"}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
