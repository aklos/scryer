/**
 * The observability feed: fetches the `get_model_health` report (completeness,
 * test anchors, the derived import graph, structural violations) and keeps it
 * current. The command runs the extractor over the whole project, so it is
 * fetched in the background — on open, when an agent run ends, and on demand
 * after a write — never on every keystroke. A refresh asked for mid-fetch
 * queues one trailing re-fetch, and an unchanged report does not re-render.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ModelHealthReport } from "../../entities/model/health";

export function useModelHealth(
  projectPath: string | null,
  /** An agent owns the model file right now — hold off, refresh when it ends. */
  busy: boolean,
): { report: ModelHealthReport | null; refresh: () => void } {
  const [report, setReport] = useState<ModelHealthReport | null>(null);
  // Serialized last report: an unchanged refresh must not re-render the app.
  const lastJson = useRef<string | null>(null);
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
      .then((r) => {
        const json = JSON.stringify(r);
        if (json === lastJson.current) return;
        lastJson.current = json;
        setReport(r);
      })
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
