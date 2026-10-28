import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** What `scryer-mcp migrate` wrote, one line per write, and what it left. */
export interface MigrationReport {
  described: string[];
  documented: string[];
  stubbed: string[];
  structure: string[];
  skipped: string[];
  /** The review checklist this run wrote, when it wrote one. */
  checklist: string | null;
}

export interface AuthoredModel {
  /** The project still holds `.scryer/model.scry`. */
  present: boolean;
  /** Open items in `.scryer/migration.md`, the review a migration leaves
   *  for an agent; null when there is no checklist. */
  openItems: number | null;
  /** The last migration's report, this session. */
  report: MigrationReport | null;
  busy: boolean;
  error: string | null;
  migrate: () => Promise<void>;
  /** Delete the old model's files; nothing reads them any more. */
  remove: () => Promise<void>;
}

/// A project from before claims lived in tests: its authored model, and
/// moving it into the code.
export function useAuthoredModel(projectPath: string | null): AuthoredModel {
  const [present, setPresent] = useState(false);
  const [openItems, setOpenItems] = useState<number | null>(null);
  const [report, setReport] = useState<MigrationReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    if (!projectPath) {
      setPresent(false);
      setOpenItems(null);
      return;
    }
    invoke<{ present: boolean; openItems: number | null }>("authored_model", { projectPath })
      .then((m) => {
        setPresent(m.present);
        setOpenItems(m.openItems ?? null);
      })
      .catch(() => setPresent(false));
  }, [projectPath]);

  useEffect(() => {
    setReport(null);
    setError(null);
    reload();
  }, [reload]);

  const run = useCallback(
    async (step: () => Promise<void>) => {
      setBusy(true);
      setError(null);
      try {
        await step();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  const migrate = useCallback(
    () =>
      run(async () => {
        if (!projectPath) return;
        setReport(await invoke<MigrationReport>("migrate_authored_model", { projectPath }));
        reload();
      }),
    [projectPath, run, reload],
  );

  const remove = useCallback(
    () =>
      run(async () => {
        if (!projectPath) return;
        await invoke("remove_authored_model", { projectPath });
        setPresent(false);
      }),
    [projectPath, run],
  );

  return { present, openItems, report, busy, error, migrate, remove };
}
