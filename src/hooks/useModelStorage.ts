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
import { stampTouches } from "../viewmodel";

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
  /** Node ids the agent created since they were last seen — highlighted on the
   *  canvas until the user selects them. */
  newNodeIds: ReadonlySet<string>;
  /** Responsibility ids the agent created since last seen — highlighted until
   *  the user selects the row. */
  newRespIds: ReadonlySet<string>;

  /** Open a project. If it has no model, status becomes `needs-model`. */
  openProject: (path: string) => Promise<void>;
  /** Create an empty model in the open project, then load it. */
  createBlankModel: (path: string) => Promise<void>;
  /** Forget the project; UI returns to the picker. */
  closeProject: () => void;
  /** Patch the model. Triggers a debounced save (suppressed during an agent run). */
  updateModel: (updater: (m: ScryModel) => ScryModel) => void;
  /** While an agent run is active, suppress the canvas write-back so the agent
   *  owns the file — layout stays client-side and is merged back on reload. */
  setAgentRunning: (running: boolean) => void;
  /** Re-read the model from disk into memory. The agent (MCP) is the
   *  authoritative writer during a build/fill — this both streams its writes in
   *  and loads the final result, WITHOUT writing our (possibly stale) in-memory
   *  model back over the agent's work. */
  reloadFromDisk: () => Promise<void>;
  /** Clear the "new" highlight for a node (the user selected it). */
  clearNewNode: (id: string) => void;
  /** Clear the "new" highlight for a responsibility (the user selected it). */
  clearNewResp: (id: string) => void;
  /** Drop a recent project from localStorage. */
  forgetRecent: (path: string) => void;
}

/// Carry placed-node layout (`cell`, and group `cell`/`size`) from the previous
/// in-memory model onto a freshly-loaded one where the load lacks it. During an
/// agent run the canvas doesn't write its layout back (the agent owns the file),
/// so reloads arrive unplaced — this merge keeps already-placed cards from
/// jumping while new nodes stay unplaced for autoLayout to position.
function mergeLayout(prev: ScryModel | null, loaded: ScryModel): ScryModel {
  if (!prev) return loaded;
  const prevCell = new Map(prev.nodes.map((n) => [n.id, n.cell] as const));
  const prevGroup = new Map(
    prev.groups.map((g) => [g.id, { cell: g.cell, size: g.size }] as const),
  );
  // Members of a group the agent left unplaced on disk (cell/size null) but that
  // the canvas already reflowed in memory: carry the in-memory member cells
  // forward instead of the stale pre-group disk cells. When the agent adds a
  // group around already-placed cards, autoLayout repacks them into the new
  // enclosure — but during an in-app build the canvas can't persist that layout,
  // so every subsequent agent write reloads the old positions and the members
  // snap back out. Pinning the reflowed positions here keeps them inside the box.
  const reflowedMembers = new Set<string>();
  for (const g of loaded.groups) {
    if (g.cell || g.size) continue; // placed on disk — disk is authoritative
    const p = prevGroup.get(g.id);
    if (p?.cell && p?.size) {
      for (const m of g.memberIds) reflowedMembers.add(m);
    }
  }
  return {
    ...loaded,
    nodes: loaded.nodes.map((n) => {
      const prior = prevCell.get(n.id);
      if (reflowedMembers.has(n.id) && prior) return { ...n, cell: prior };
      return n.cell ? n : prior ? { ...n, cell: prior } : n;
    }),
    groups: loaded.groups.map((g) => {
      const p = prevGroup.get(g.id);
      if (!p) return g;
      return { ...g, cell: g.cell ?? p.cell, size: g.size ?? p.size };
    }),
  };
}

/// Ids the agent introduced since the previous model, for review highlighting.
/// Only the node/responsibility ids present in `loaded` but absent in `prev`.
function arrivals(prev: ScryModel, loaded: ScryModel) {
  const prevNodes = new Set(prev.nodes.map((n) => n.id));
  const newNodes: string[] = [];
  for (const n of loaded.nodes) if (!prevNodes.has(n.id)) newNodes.push(n.id);
  const prevResps = new Set<string>();
  for (const n of prev.nodes)
    for (const r of n.responsibilities ?? []) prevResps.add(r.id);
  const newResps: { id: string; nodeId: string }[] = [];
  for (const n of loaded.nodes)
    for (const r of n.responsibilities ?? [])
      if (!prevResps.has(r.id)) newResps.push({ id: r.id, nodeId: n.id });
  return { newNodes, newResps };
}

/// Add `add` ids to a tracked set and prune any no longer in `keep` (the agent
/// deleted them). Returns the original set reference when nothing changed so the
/// state update is a no-op.
function accumulate(
  cur: ReadonlySet<string>,
  add: string[],
  keep: ReadonlySet<string>,
): ReadonlySet<string> {
  const next = new Set(cur);
  for (const id of add) next.add(id);
  for (const id of cur) if (!keep.has(id)) next.delete(id);
  if (next.size === cur.size) {
    let same = true;
    for (const id of next) if (!cur.has(id)) { same = false; break; }
    if (same) return cur;
  }
  return next;
}

export function useModelStorage(): ModelStorage {
  const [status, setStatus] = useState<ProjectStatus>("idle");
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [modelRef, setModelRef] = useState<string | null>(null);
  const [model, setModel] = useState<ScryModel | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recentProjects, setRecentProjects] = useState<string[]>(() => readRecent());
  // Ids the agent has introduced (and not yet reviewed). Frontend-only — diffed
  // from each external/agent write, cleared when the user selects the item.
  const [newNodeIds, setNewNodeIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [newRespIds, setNewRespIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Exact bytes of our last write. A `model-changed` event whose file content
  // matches this is our own echo and is ignored; anything else (an agent via
  // MCP) is a real external change and gets loaded. Content-based rather than a
  // write counter so inotify event coalescing can't make us miss reloads.
  const lastWrittenRaw = useRef<string | null>(null);
  // True while an agent session is writing the model; suppresses the canvas
  // write-back so the agent owns the file and the two don't clobber.
  const agentRunningRef = useRef(false);
  // Latest model / ref, read by callbacks without re-creating them.
  const modelStateRef = useRef<ScryModel | null>(model);
  modelStateRef.current = model;
  const modelRefRef = useRef<string | null>(modelRef);
  modelRefRef.current = modelRef;
  // Mirror of newNodeIds for synchronous reads inside applyLoadedRaw — whether a
  // responsibility earns its own "new" row-tint depends on whether its owning
  // node is currently flagged new (the node's ring already covers it).
  const newNodeIdsRef = useRef<ReadonlySet<string>>(newNodeIds);
  newNodeIdsRef.current = newNodeIds;

  // Apply a freshly-read model file to in-memory state: flag the ids the agent
  // introduced (and prune removed ones) for review highlighting, remember the
  // bytes so the watcher doesn't echo them, and diff-merge layout so already-
  // placed cards keep their positions while new nodes stay unplaced for
  // autoLayout to seed. Callers dedup on `raw === lastWrittenRaw.current` first.
  const applyLoadedRaw = useCallback((raw: string) => {
    const loaded = JSON.parse(raw) as ScryModel;
    const prevModel = modelStateRef.current;
    if (prevModel) {
      const { newNodes, newResps } = arrivals(prevModel, loaded);
      const keepNodes = new Set(loaded.nodes.map((n) => n.id));
      const keepResps = new Set<string>();
      for (const n of loaded.nodes)
        for (const r of n.responsibilities ?? []) keepResps.add(r.id);
      // A whole new node's ring already announces everything inside it as new. A
      // responsibility only earns its own row-tint when it lands on a node the
      // user has already reviewed — rows on a still-new node are covered by the
      // ring, and would pop into view the instant the user selects the card to
      // dismiss it (the ring clears, but the masked rows don't). Drop rows owned
      // by a still-flagged-new node (this batch's arrivals ∪ ids still pending
      // review) at the source, so dismissing a card dismisses the whole card.
      const flaggedNew = new Set<string>(newNodes);
      for (const id of newNodeIdsRef.current) flaggedNew.add(id);
      const reviewableResps = newResps
        .filter((r) => !flaggedNew.has(r.nodeId))
        .map((r) => r.id);
      setNewNodeIds((cur) => accumulate(cur, newNodes, keepNodes));
      setNewRespIds((cur) => accumulate(cur, reviewableResps, keepResps));
    }
    lastWrittenRaw.current = raw;
    setModel((prev) => mergeLayout(prev, loaded));
  }, []);

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
          applyLoadedRaw(raw);
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
  }, [modelRef, applyLoadedRaw]);

  const openProject = useCallback(async (path: string) => {
    setStatus("loading");
    setError(null);
    setProjectPath(path);
    // The model just loaded is the review baseline — nothing is "new" yet.
    setNewNodeIds(new Set());
    setNewRespIds(new Set());
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
    setNewNodeIds(new Set());
    setNewRespIds(new Set());
  }, []);

  const clearNewNode = useCallback((id: string) => {
    setNewNodeIds((cur) => {
      if (!cur.has(id)) return cur;
      const next = new Set(cur);
      next.delete(id);
      return next;
    });
  }, []);
  const clearNewResp = useCallback((id: string) => {
    setNewRespIds((cur) => {
      if (!cur.has(id)) return cur;
      const next = new Set(cur);
      next.delete(id);
      return next;
    });
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
        const edited = updater(cur);
        if (edited === cur) return cur;
        // Date any responsibility/property whose truth changed in this edit, so
        // the fossilization clock advances for canvas edits. This is the canvas
        // mirror of the agent's Rust-side write_model_at stamping; doing it at
        // this single chokepoint covers every edit path (granular, EditModal
        // bulk-commit, auto-"changed") and a layout-only edit re-dates nothing.
        const next = stampTouches(cur, edited);
        if (saveTimer.current) clearTimeout(saveTimer.current);
        const ref = modelRef;
        const serialized = JSON.stringify(next, null, 2);
        saveTimer.current = setTimeout(() => {
          // During an agent run the agent owns the file; keep layout client-side
          // (autoLayout re-seeds and persists it after the run ends, once we've
          // reloaded the agent's model from disk) so we don't clobber writes.
          if (agentRunningRef.current) return;
          lastWrittenRaw.current = serialized;
          invoke("write_model", { refStr: ref, data: serialized }).catch(() => {});
        }, SAVE_DEBOUNCE_MS);
        return next;
      });
    },
    [modelRef],
  );

  const setAgentRunning = useCallback((running: boolean) => {
    agentRunningRef.current = running;
  }, []);

  // The agent (MCP) is the authoritative writer during a build/fill. Reloading
  // from disk both streams its writes in and loads the final result. We must
  // NEVER write the in-memory model back at the end of an agent run: if a live
  // reload was missed the in-memory model is stale/empty and the write would
  // clobber the agent's work (only the separate baseline file would survive).
  const reloadFromDisk = useCallback(async () => {
    const ref = modelRefRef.current;
    if (!ref) return;
    try {
      const raw = await invoke<string>("read_model", { refStr: ref });
      if (raw === lastWrittenRaw.current) return; // unchanged since last load/write
      applyLoadedRaw(raw);
    } catch {
      /* transient — ignore */
    }
  }, [applyLoadedRaw]);

  return {
    status,
    projectPath,
    modelRef,
    model,
    error,
    recentProjects,
    newNodeIds,
    newRespIds,
    openProject,
    createBlankModel,
    closeProject,
    updateModel,
    setAgentRunning,
    reloadFromDisk,
    clearNewNode,
    clearNewResp,
    forgetRecent,
  };
}
