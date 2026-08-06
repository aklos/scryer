import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  resolveLaunch,
  SUBAGENT_DEFAULTS,
  type Detected,
  type ResolvedLaunch,
  type SubagentSettings,
} from "../SettingsPanel";

export interface LaunchSettings {
  /** Which agent + model + effort a fill will actually run with. */
  launch: ResolvedLaunch;
  /** Whether to confirm before launching an agent (the "don't ask again" gate). */
  confirmLaunch: boolean;
  /** Re-read settings + detected tools from disk (e.g. after the panel closes). */
  reload: () => void;
  /** Persist confirmLaunch=false — the user chose "don't ask again". */
  clearConfirm: () => Promise<void>;
}

/// Loads the subagent settings + detected agents and resolves the launch setup,
/// shared by the powerline readout and the pre-launch confirm gate so they can
/// never disagree about what a fill will run. Both the picker and the workspace
/// hold their own instance — they're mutually exclusive subtrees, so only one is
/// ever mounted.
export function useLaunchSettings(): LaunchSettings {
  const [subagent, setSubagent] = useState<SubagentSettings>(SUBAGENT_DEFAULTS);
  const [detected, setDetected] = useState<Detected>({
    claude: false,
    codex: false,
    copilot: false,
  });

  const reload = useCallback(() => {
    invoke<SubagentSettings>("get_subagent_settings")
      .then((s) => setSubagent({ ...SUBAGENT_DEFAULTS, ...s }))
      .catch(() => {});
    invoke<Detected>("detect_ai_tools", { projectPath: null })
      .then((d) => setDetected({ claude: !!d.claude, codex: !!d.codex, copilot: !!d.copilot }))
      .catch(() => {});
  }, []);
  useEffect(reload, [reload]);

  const launch = useMemo(() => resolveLaunch(subagent, detected), [subagent, detected]);

  const clearConfirm = useCallback(async () => {
    const next = { ...subagent, confirmLaunch: false };
    setSubagent(next);
    try {
      await invoke("set_subagent_settings", { settings: next });
    } catch {
      /* best-effort — the gate just stays on if the write fails */
    }
  }, [subagent]);

  return { launch, confirmLaunch: subagent.confirmLaunch, reload, clearConfirm };
}
