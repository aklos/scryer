/**
 * The claim test-verdict feed: fetches `get_test_statuses` (each claim's last
 * recorded outcome, re-verified against the working tree) and keeps it
 * current, alongside `get_probe_statuses` (what each claim's attached test
 * caught when deliberately broken). Refreshes on open, on every busy→idle edge
 * (an in-app agent run ended), and whenever the `.test-results.json` cache
 * changes on disk — an agent over MCP ingesting a report or ending a probe
 * mid-session lights the badges live.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  probeResultsOf,
  testVerdictsOf,
  type ClaimProbeStatus,
  type ClaimTestStatus,
} from "../health";

export function useTestStatuses(
  projectPath: string | null,
  /** An agent owns the model file right now — hold off, refresh when it ends. */
  busy: boolean,
): {
  verdicts: Record<string, ClaimTestStatus>;
  probes: Record<string, ClaimProbeStatus>;
  refresh: () => void;
} {
  const [verdicts, setVerdicts] = useState<Record<string, ClaimTestStatus>>({});
  const [probes, setProbes] = useState<Record<string, ClaimProbeStatus>>({});
  const inFlight = useRef(false);
  const queued = useRef(false);

  const fetchStatuses = useCallback(() => {
    if (!projectPath) return;
    if (inFlight.current) {
      queued.current = true;
      return;
    }
    inFlight.current = true;
    // Verdicts and probes come off the same cache file and the same refresh
    // edges, so they travel together — a probe mark beside a verdict from a
    // different moment would be worse than no mark at all.
    Promise.all([
      invoke<ClaimTestStatus[]>("get_test_statuses", { cwd: projectPath })
        .then((s) => setVerdicts(testVerdictsOf(Array.isArray(s) ? s : [])))
        .catch(() => {}),
      invoke<ClaimProbeStatus[]>("get_probe_statuses", { cwd: projectPath })
        .then((s) => setProbes(probeResultsOf(Array.isArray(s) ? s : [])))
        .catch(() => {}),
    ])
      .finally(() => {
        inFlight.current = false;
        if (queued.current) {
          queued.current = false;
          fetchStatuses();
        }
      });
  }, [projectPath]);

  // On open, and on every busy→idle edge.
  useEffect(() => {
    if (busy) return;
    fetchStatuses();
  }, [busy, fetchStatuses]);

  // Live edge: the watcher emits when the on-disk cache changes (an external
  // agent ingested a report). Verdict data is cheap — refresh immediately.
  useEffect(() => {
    if (!projectPath) return;
    const un = listen("test-results-changed", () => fetchStatuses());
    return () => {
      void un.then((f) => f());
    };
  }, [projectPath, fetchStatuses]);

  return { verdicts, probes, refresh: fetchStatuses };
}
