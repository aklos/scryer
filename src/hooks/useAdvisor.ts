import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { C4ModelData, C4Node, C4Edge, StartingLevel, SourceLocation, Hint } from "../types";

interface UseAdvisorParams {
  nodes: C4Node[];
  edges: C4Edge[];
  startingLevel: StartingLevel;
  sourceMap: Record<string, SourceLocation[]>;
}

export function useAdvisor({ nodes, edges, startingLevel, sourceMap }: UseAdvisorParams) {
  const [hints, setHints] = useState<Record<string, Hint[]>>({});
  const [hintLoading, setHintLoading] = useState(false);
  const [dismissedHints, setDismissedHints] = useState<Set<string>>(new Set());

  const [aiConfigured, setAiConfigured] = useState(false);
  const [aiEnabled, setAiEnabled] = useState(() => localStorage.getItem("scryer:aiEnabled") !== "false");
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Check AI settings on mount
  useEffect(() => {
    invoke<{ configured: boolean }>("get_ai_settings")
      .then((s) => setAiConfigured(s.configured))
      .catch(() => setAiConfigured(false));
  }, []);

  // Persist AI enabled toggle
  useEffect(() => {
    localStorage.setItem("scryer:aiEnabled", String(aiEnabled));
  }, [aiEnabled]);

  // Manually trigger AI review
  const fetchHints = useCallback(async () => {
    if (nodes.length === 0 || !aiConfigured || !aiEnabled) return;
    setHintLoading(true);
    try {
      const modelData: C4ModelData = { nodes, edges, startingLevel, sourceMap };
      const raw = await invoke<string>("get_hints", { data: JSON.stringify(modelData) });
      const list: Hint[] = JSON.parse(raw);
      const grouped: Record<string, Hint[]> = {};
      for (const h of list) {
        (grouped[h.nodeId] ??= []).push(h);
      }
      setHints(grouped);
    } catch {
      setHints({});
    }
    setHintLoading(false);
  }, [nodes, edges, startingLevel, sourceMap, aiConfigured, aiEnabled]);

  // Filter out dismissed hints
  const activeHints = useMemo(() => {
    if (dismissedHints.size === 0) return hints;
    const filtered: Record<string, Hint[]> = {};
    for (const [nodeId, nodeHints] of Object.entries(hints)) {
      const kept = nodeHints.filter((h) => !dismissedHints.has(`${h.nodeId}:${h.message}`));
      if (kept.length > 0) filtered[nodeId] = kept;
    }
    return filtered;
  }, [hints, dismissedHints]);

  const dismissHint = useCallback((hint: Hint) => {
    setDismissedHints((prev) => new Set(prev).add(`${hint.nodeId}:${hint.message}`));
  }, []);

  return {
    hints: activeHints,
    hintLoading,
    fetchHints,
    dismissHint,
    aiConfigured,
    setAiConfigured,
    aiEnabled,
    setAiEnabled,
    settingsOpen,
    setSettingsOpen,
  };
}
