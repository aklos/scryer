/**
 * The observability feed: fetches the `get_model_health` report (coverage,
 * anchor fingerprints, link evidence) and keeps it current. The command runs
 * the extractor over the whole project, so it's fetched in the background —
 * never on every keystroke — and refreshed when an agent run finishes or a
 * reconcile changes what the anchors mean.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ModelHealthReport } from "../health";

export function useModelHealth(
  projectPath: string | null,
  /** An agent owns the model file right now — hold off, refresh when it ends. */
  busy: boolean,
): { report: ModelHealthReport | null; refresh: () => void } {
  const [report, setReport] = useState<ModelHealthReport | null>(null);
  const inFlight = useRef(false);
  // A refresh issued mid-fetch used to be DROPPED — the caller had a reason
  // (a verdict just changed what the anchors mean), so queue one trailing
  // re-fetch and run it when the current one lands.
  const queued = useRef(false);

  const fetchReport = useCallback(() => {
    if (!projectPath) return;
    if (inFlight.current) {
      queued.current = true;
      return;
    }
    inFlight.current = true;
    invoke<ModelHealthReport>("get_model_health", { cwd: projectPath })
      .then((r) => setReport(r))
      .catch(() => {})
      .finally(() => {
        inFlight.current = false;
        if (queued.current) {
          queued.current = false;
          fetchReport();
        }
      });
  }, [projectPath]);

  // On open, and on every busy→idle edge (agent finished writing the model).
  useEffect(() => {
    if (busy) return;
    fetchReport();
  }, [busy, fetchReport]);

  return { report, refresh: fetchReport };
}
