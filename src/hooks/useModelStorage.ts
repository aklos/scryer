/**
 * Project-local model storage. One project open at a time; the model lives on
 * disk at `{projectPath}/.scryer/model.scry`.
 *
 * Responsibilities:
 *  - load / save the model via Tauri commands (`read_model` / `write_model`)
 *  - debounce saves so a burst of canvas edits collapses to one write
 *  - subscribe to `model-changed` so external writes (an agent via MCP) are
 *    picked up live; suppress the event for our own writes
 *  - track recent projects in localStorage so the picker has somewhere to start
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ScryModel } from "../viewmodel";

const RECENT_KEY = "scryer:recent-projects";
const RECENT_CAP = 8;
const SAVE_DEBOUNCE_MS = 500;

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

export type ProjectStatus =
  | "idle" // no project open
  | "loading"
  | "ready"
  | "needs-model" // project chosen, no .scry yet
  | "legacy" // .scry exists but wrong schema version
  | "error";

export interface ModelStorage {
  status: ProjectStatus;
  projectPath: string | null;
  modelRef: string | null;
  model: ScryModel | null;
  error: string | null;
  recentProjects: string[];

  /** Open a project. If it has no model, status becomes `needs-model`. */
  openProject: (path: string) => Promise<void>;
  /** Create an empty model in the open project, then load it. */
  createBlankModel: (path: string) => Promise<void>;
  /** Forget the project; UI returns to the picker. */
  closeProject: () => void;
  /** Patch the model. Triggers a debounced save. */
  updateModel: (updater: (m: ScryModel) => ScryModel) => void;
  /** Drop a recent project from localStorage. */
  forgetRecent: (path: string) => void;
}

export function useModelStorage(): ModelStorage {
  const [status, setStatus] = useState<ProjectStatus>("idle");
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [modelRef, setModelRef] = useState<string | null>(null);
  const [model, setModel] = useState<ScryModel | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recentProjects, setRecentProjects] = useState<string[]>(() => readRecent());

  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Exact bytes of our last write. A `model-changed` event whose file content
  // matches this is our own echo and is ignored; anything else (an agent via
  // MCP) is a real external change and gets loaded. Content-based rather than a
  // write counter so inotify event coalescing can't make us miss reloads.
  const lastWrittenRaw = useRef<string | null>(null);

  // File-watcher subscription — re-read model from disk on external writes.
  useEffect(() => {
    if (!modelRef) return;
    let unlisten: (() => void) | null = null;
    let active = true;
    (async () => {
      try {
        await invoke("watch_project", { refStr: modelRef });
      } catch {
        /* watch_project failure is non-fatal */
      }
      const off = await listen<string>("model-changed", async (event) => {
        if (event.payload !== modelRef) return;
        try {
          const raw = await invoke<string>("read_model", { refStr: modelRef });
          if (!active) return;
          if (raw === lastWrittenRaw.current) return; // our own write echoed back
          const loaded = JSON.parse(raw) as ScryModel;
          // Positions are seeded client-side after card measurement
          // (see autoLayout), not here — we have no DOM to measure against.
          setModel(loaded);
        } catch {
          /* transient — ignore */
        }
      });
      if (active) unlisten = off;
      else off();
    })();
    return () => {
      active = false;
      unlisten?.();
    };
  }, [modelRef]);

  const openProject = useCallback(async (path: string) => {
    setStatus("loading");
    setError(null);
    setProjectPath(path);
    try {
      const isLegacy = await invoke<boolean>("is_legacy_model", {
        projectPath: path,
      });
      if (isLegacy) {
        setStatus("legacy");
        setError(
          "This project's `.scryer/model.scry` was created by an older scryer (pre-0.3). v0.3 is not backward compatible.",
        );
        setModelRef(null);
        setModel(null);
        return;
      }
      const ref = `project:${path}`;
      try {
        const raw = await invoke<string>("read_model", { refStr: ref });
        const loaded = JSON.parse(raw) as ScryModel;
        // Positions are seeded client-side after card measurement
        // (see autoLayout), not here — we have no DOM to measure against.
        const next = bumpRecent(path);
        writeRecent(next);
        setRecentProjects(next);
        setModelRef(ref);
        setModel(loaded);
        setStatus("ready");
      } catch {
        // No model yet — caller can choose to create blank or generate.
        setModelRef(null);
        setModel(null);
        setStatus("needs-model");
      }
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  }, []);

  const createBlankModel = useCallback(
    async (path: string) => {
      setStatus("loading");
      setError(null);
      try {
        await invoke("create_blank_model", { projectPath: path });
        await openProject(path);
      } catch (e) {
        setStatus("error");
        setError(String(e));
      }
    },
    [openProject],
  );

  const closeProject = useCallback(() => {
    if (saveTimer.current) {
      clearTimeout(saveTimer.current);
      saveTimer.current = null;
    }
    setStatus("idle");
    setProjectPath(null);
    setModelRef(null);
    setModel(null);
    setError(null);
  }, []);

  const forgetRecent = useCallback((path: string) => {
    const next = readRecent().filter((p) => p !== path);
    writeRecent(next);
    setRecentProjects(next);
  }, []);

  const updateModel = useCallback(
    (updater: (m: ScryModel) => ScryModel) => {
      setModel((cur) => {
        if (!cur || !modelRef) return cur;
        const next = updater(cur);
        if (next === cur) return cur;
        if (saveTimer.current) clearTimeout(saveTimer.current);
        const ref = modelRef;
        const serialized = JSON.stringify(next, null, 2);
        saveTimer.current = setTimeout(() => {
          lastWrittenRaw.current = serialized;
          invoke("write_model", { refStr: ref, data: serialized }).catch(() => {});
        }, SAVE_DEBOUNCE_MS);
        return next;
      });
    },
    [modelRef],
  );

  return {
    status,
    projectPath,
    modelRef,
    model,
    error,
    recentProjects,
    openProject,
    createBlankModel,
    closeProject,
    updateModel,
    forgetRecent,
  };
}
