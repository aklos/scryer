/**
 * The open project's tree, read from its code, and the tree at the base its
 * changes are read against. One project open at a time.
 *
 * Claims:
 *  - load the tree (`read_planned`), every claim todo tests included
 *  - read where its changes start (`read_change_base`: HEAD on the default
 *    branch, the merge base off it) and the tree at that commit
 *    (`read_base_model`), which the backend derives once per commit; the
 *    base is fetched again only when its commit moves
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

/** Where the project's changes are read from. */
export interface ChangeBase {
  commit: string;
  /** The branch the checkout diverged from; absent when the base is HEAD. */
  against?: string;
  /** Commits ahead of `against`. */
  ahead: number;
  /** Something is uncommitted. */
  dirty: boolean;
}

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
  /** The tree: every claim, todo tests included. */
  model: ScryModel | null;
  /** The tree at `changeBase`; null outside git or until it is read. */
  base: ScryModel | null;
  /** Where the changes are read from; null outside a git repository. */
  changeBase: ChangeBase | null;
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
  /** Re-read the tree and its base; true when the tree changed. */
  reloadFromDisk: () => Promise<boolean>;
  /** Show an edit over the tree until the re-read after `write` lands,
   *  so the page never flashes back to the old text meanwhile. */
  showEdit: (apply: (m: ScryModel) => ScryModel, write: Promise<unknown>) => Promise<void>;
  /** Drop a recent project from localStorage. */
  forgetRecent: (path: string) => void;
}

export function useModelStorage(): ModelStorage {
  const [status, setStatus] = useState<ProjectStatus>("idle");
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [modelRef, setModelRef] = useState<string | null>(null);
  const [model, setModel] = useState<ScryModel | null>(null);
  const [base, setBase] = useState<ScryModel | null>(null);
  const [changeBase, setChangeBase] = useState<ChangeBase | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [externalGeneration, setExternalGeneration] = useState(0);
  const [recentProjects, setRecentProjects] = useState<string[]>(() => readRecent());
  // Edits written but not yet re-read, applied in order over the disk layers.
  const [shown, setShown] = useState<readonly { apply: (m: ScryModel) => ScryModel }[]>([]);

  // The bytes last applied: an unchanged re-read must not re-render.
  const plannedRaw = useRef<string | null>(null);
  // The base commit last read, and the one being read.
  const baseCommit = useRef<string | null>(null);
  const modelRefRef = useRef<string | null>(null);
  modelRefRef.current = modelRef;

  // Where the changes start, and the tree there when that commit moved. A
  // base read for a project since closed or switched is dropped.
  const readBase = useCallback(async (ref: string) => {
    const at = await invoke<ChangeBase | null>("read_change_base", { refStr: ref }).catch(() => undefined);
    if (at === undefined || modelRefRef.current !== ref) return;
    setChangeBase((prev) =>
      prev && at && prev.commit === at.commit && prev.against === at.against && prev.ahead === at.ahead && prev.dirty === at.dirty
        ? prev
        : at,
    );
    const commit = at?.commit ?? null;
    if (commit === baseCommit.current) return;
    baseCommit.current = commit;
    if (!commit) {
      setBase(null);
      return;
    }
    const json = await invoke<string>("read_base_model", { refStr: ref, commit }).catch(() => null);
    if (modelRefRef.current !== ref || baseCommit.current !== commit) return;
    if (json === null) baseCommit.current = null;
    else setBase(JSON.parse(json) as ScryModel);
  }, []);

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
    } catch {
      /* transient — the next re-read tries again */
    }
    if (changed) setExternalGeneration((g) => g + 1);
    void readBase(ref);
    return changed;
  }, [readBase]);

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

  const showEdit = useCallback(
    async (apply: (m: ScryModel) => ScryModel, write: Promise<unknown>) => {
      const edit = { apply };
      setShown((s) => [...s, edit]);
      await write.catch(() => {});
      await reloadFromDisk();
      // A failed write re-read the old text, so dropping the edit reverts it.
      setShown((s) => s.filter((e) => e !== edit));
    },
    [reloadFromDisk],
  );

  const openProject = useCallback(async (path: string, opts?: { remember?: boolean }) => {
    setStatus("loading");
    setError(null);
    setProjectPath(path);
    const ref = `project:${path}`;
    try {
      const planned = await invoke<string>("read_planned", { refStr: ref });
      plannedRaw.current = planned;
      baseCommit.current = null;
      setBase(null);
      setChangeBase(null);
      if (opts?.remember !== false) {
        const next = bumpRecent(path);
        writeRecent(next);
        setRecentProjects(next);
      }
      setModelRef(ref);
      setModel(JSON.parse(planned) as ScryModel);
      setStatus("ready");
      modelRefRef.current = ref;
      void readBase(ref);
    } catch (e) {
      setModelRef(null);
      setModel(null);
      setStatus("error");
      setError(String(e));
    }
  }, [readBase]);

  const closeProject = useCallback(() => {
    setStatus("idle");
    setProjectPath(null);
    setModelRef(null);
    setModel(null);
    setBase(null);
    setChangeBase(null);
    setError(null);
    plannedRaw.current = null;
    baseCommit.current = null;
    setShown([]);
  }, []);

  const forgetRecent = useCallback((path: string) => {
    const next = readRecent().filter((p) => p !== path);
    writeRecent(next);
    setRecentProjects(next);
  }, []);

  const shownModel = useMemo(() => model && shown.reduce((m, e) => e.apply(m), model), [model, shown]);

  return {
    status,
    projectPath,
    modelRef,
    model: shownModel,
    base,
    changeBase,
    error,
    recentProjects,
    externalGeneration,
    openProject,
    closeProject,
    reloadFromDisk,
    showEdit,
    forgetRecent,
  };
}
