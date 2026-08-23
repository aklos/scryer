/**
 * The claim test-verdict feed: fetches `get_test_statuses` (each claim's last
 * recorded outcome, re-verified against the working tree) and keeps it
 * current. Refreshes on open, on every busy→idle edge (an in-app agent run
 * ended), and whenever the `.test-results.json` cache changes on disk — an
 * agent over MCP ingesting a report mid-session lights the badges live.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { testVerdictsOf, type ClaimTestStatus } from "../health";

export function useTestStatuses(
  projectPath: string | null,
  /** An agent owns the model file right now — hold off, refresh when it ends. */
  busy: boolean,
): { verdicts: Record<string, ClaimTestStatus>; refresh: () => void } {
  const [verdicts, setVerdicts] = useState<Record<string, ClaimTestStatus>>({});
  const inFlight = useRef(false);
  const queued = useRef(false);

  const fetchStatuses = useCallback(() => {
    if (!projectPath) return;
    if (inFlight.current) {
      queued.current = true;
      return;
    }
    inFlight.current = true;
    invoke<ClaimTestStatus[]>("get_test_statuses", { cwd: projectPath })
      .then((s) => setVerdicts(testVerdictsOf(Array.isArray(s) ? s : [])))
      .catch(() => {})
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

  return { verdicts, refresh: fetchStatuses };
}
