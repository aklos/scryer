/**
 * The open project's tree, read from its code. One project open at a time.
 * The backend derives the C4 tree from packaging and tests and serves it in
 * two layers: `committed` (implemented claims) and `planned` (plus the todo
 * stubs), so `planDiff(committed, planned)` is the work still planned.
 *
 * Responsibilities:
 *  - load both layers (`read_planned` / `read_model`)
 *  - re-read them when the watcher reports a fresh derivation or a changed
 *    `structure.toml`, and every few seconds, since the tree follows the code
 *    and the watcher only covers `.scryer/`
 *  - track recent projects in localStorage so the picker has somewhere to start
 *
 * Nothing is written from here: edits go to the code (claims into test
 * files, display names into structure.toml) through the derived editor.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ScryModel } from "../../entities/model/viewmodel";
import { EMPTY_DIFF, planDiff as computePlanDiff, type ModelDiff } from "../../entities/model/planDiff";

const RECENT_KEY = "scryer:recent-projects";
const RECENT_CAP = 8;
/** How often the tree is re-read: it follows the code, which no watcher here
 *  covers. A re-read of an unchanged project costs a file walk, not a parse. */
const POLL_MS = 4000;

function readRecent(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((p): p is string => typeof p === "string");
  } catch {
    return [];
  }
}

function writeRecent(paths: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(paths.slice(0, RECENT_CAP)));
  } catch {
    /* localStorage unavailable — silently ignore */
  }
}

function bumpRecent(path: string): string[] {
  const all = readRecent().filter((p) => p !== path);
  all.unshift(path);
  return all.slice(0, RECENT_CAP);
}

export type ProjectStatus = "idle" | "loading" | "ready" | "error";

export interface ModelStorage {
  status: ProjectStatus;
  projectPath: string | null;
  modelRef: string | null;
  /** The planned layer: every claim, todo stubs included. */
  model: ScryModel | null;
  /** The committed layer: implemented claims only. */
  committed: ScryModel | null;
  /** `diff(committed, planned)` — the todo stubs, as planned marks. */
  planDiff: ModelDiff;
  error: string | null;
  recentProjects: string[];
  /** Bumped each time a re-read changed the tree — the signal for consumers
   *  whose data is not the tree itself (health, status) to re-fetch. */
  externalGeneration: number;

  /** Open a project; `remember: false` keeps it out of the recent list,
   *  as for a switch to another worktree of the open one. */
  openProject: (path: string, opts?: { remember?: boolean }) => Promise<void>;
  /** Forget the project; UI returns to the picker. */
  closeProject: () => void;
  /** Re-read both layers; true when either changed. */
  reloadFromDisk: () => Promise<boolean>;
  /** Drop a recent project from localStorage. */
  forgetRecent: (path: string) => void;
}

export function useModelStorage(): ModelStorage {
  const [status, setStatus] = useState<ProjectStatus>("idle");
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [modelRef, setModelRef] = useState<string | null>(null);
  const [model, setModel] = useState<ScryModel | null>(null);
  const [committed, setCommitted] = useState<ScryModel | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [externalGeneration, setExternalGeneration] = useState(0);
  const [recentProjects, setRecentProjects] = useState<string[]>(() => readRecent());

  // The bytes last applied per layer: an unchanged re-read must not re-render.
  const plannedRaw = useRef<string | null>(null);
  const committedRaw = useRef<string | null>(null);
  const modelRefRef = useRef<string | null>(null);
  modelRefRef.current = modelRef;

  const reloadFromDisk = useCallback(async (): Promise<boolean> => {
    const ref = modelRefRef.current;
    if (!ref) return false;
    let changed = false;
    try {
      const planned = await invoke<string>("read_planned", { refStr: ref });
      if (planned !== plannedRaw.current) {
        plannedRaw.current = planned;
        setModel(JSON.parse(planned) as ScryModel);
        changed = true;
      }
      const base = await invoke<string>("read_model", { refStr: ref });
      if (base !== committedRaw.current) {
        committedRaw.current = base;
        setCommitted(JSON.parse(base) as ScryModel);
        changed = true;
      }
    } catch {
      /* transient — the next re-read tries again */
    }
    if (changed) setExternalGeneration((g) => g + 1);
    return changed;
  }, []);

  // The watcher reports a fresh derivation or an edited structure.toml.
  useEffect(() => {
    if (!modelRef) return;
    let unlisten: (() => void) | null = null;
    let active = true;
    void (async () => {
      await invoke("watch_project", { refStr: modelRef }).catch(() => {});
      const off = await listen<string>("model-changed", (event) => {
        if (event.payload === modelRef) void reloadFromDisk();
      });
      if (active) unlisten = off;
      else off();
    })();
    const poll = setInterval(() => void reloadFromDisk(), POLL_MS);
    return () => {
      active = false;
      unlisten?.();
      clearInterval(poll);
    };
  }, [modelRef, reloadFromDisk]);

  const openProject = useCallback(async (path: string, opts?: { remember?: boolean }) => {
    setStatus("loading");
    setError(null);
    setProjectPath(path);
    const ref = `project:${path}`;
    try {
      const planned = await invoke<string>("read_planned", { refStr: ref });
      const base = await invoke<string>("read_model", { refStr: ref });
      plannedRaw.current = planned;
      committedRaw.current = base;
      if (opts?.remember !== false) {
        const next = bumpRecent(path);
        writeRecent(next);
        setRecentProjects(next);
      }
      setModelRef(ref);
      setModel(JSON.parse(planned) as ScryModel);
      setCommitted(JSON.parse(base) as ScryModel);
      setStatus("ready");
    } catch (e) {
      setModelRef(null);
      setModel(null);
      setCommitted(null);
      setStatus("error");
      setError(String(e));
    }
  }, []);

  const closeProject = useCallback(() => {
    setStatus("idle");
    setProjectPath(null);
    setModelRef(null);
    setModel(null);
    setCommitted(null);
    setError(null);
    plannedRaw.current = null;
    committedRaw.current = null;
  }, []);

  const forgetRecent = useCallback((path: string) => {
    const next = readRecent().filter((p) => p !== path);
    writeRecent(next);
    setRecentProjects(next);
  }, []);

  const planDiff = useMemo<ModelDiff>(
    () => (committed && model ? computePlanDiff(committed, model) : EMPTY_DIFF),
    [committed, model],
  );

  return {
    status,
    projectPath,
    modelRef,
    model,
    committed,
    planDiff,
    error,
    recentProjects,
    externalGeneration,
    openProject,
    closeProject,
    reloadFromDisk,
    forgetRecent,
  };
}
