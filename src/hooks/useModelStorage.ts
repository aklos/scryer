/**
 * Project-local model storage. One project open at a time. Two layers live on
 * disk under `{projectPath}/.scryer/`: the committed `model.scry` (source of
 * truth — what the code is believed to satisfy) and the `planned.scry` draft
 * (the intent the canvas and agent edit). The canvas works the PLANNED layer;
 * the committed model is the diff base and advances only when work is folded in
 * (the agent's `mark_implemented`) or extracted from code.
 *
 * Responsibilities:
 *  - load both layers (`read_planned` working / `read_model` committed)
 *  - save canvas edits to the plan (`write_planned`), debounced so a burst of
 *    edits collapses to one write
 *  - derive the live plan diff `diff(committed, planned)` for the UI
 *  - subscribe to `model-changed` so external writes (an agent via MCP) to
 *    either layer are picked up live; suppress the event for our own writes
 *  - track recent projects in localStorage so the picker has somewhere to start
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Node, Responsibility, ScryModel } from "../viewmodel";
import { respTruthChanged, stampTouches } from "../viewmodel";
import { EMPTY_DIFF, planDiff as computePlanDiff, type ModelDiff } from "../planDiff";
import type { HistoryEvent } from "../history";

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

/** One field's before → after, for `changed` items. */
export interface FieldDiff {
  field: string;
  from: string;
  to: string;
}

/** One line of a change revision — something the agent added/changed/removed. */
export interface ChangeItem {
  op: "added" | "changed" | "removed";
  what: "node" | "claim" | "link";
  /** Node name, claim statement, or "A → B" for links. */
  label: string;
  /** Host node name, for claims. */
  context?: string;
  /** Jump target — absent for removals. */
  nodeId?: string;
  /** Per-field value diffs, for `changed` items — the revision detail. */
  fields?: FieldDiff[];
}

/** One edit burst (an agent write, or a user commit), diffed. Session-local. */
export interface ChangeRevision {
  at: number;
  /** Who made the edit — the agent (external write) or the user (UI commit). */
  by: "agent" | "user";
  items: ChangeItem[];
}

/** Consecutive user commits inside this window merge into one revision, so a
 *  single logical edit (which may land as several editor intents) reads as
 *  one entry. */
const USER_MERGE_WINDOW_MS = 3000;

const CHANGE_LOG_CAP = 200;

/** Render a field value for the revision diff: absence reads as "—". */
function fmtFieldValue(v: unknown): string {
  if (v === undefined || v === null || v === "") return "—";
  if (typeof v === "boolean") return v ? "yes" : "no";
  if (Array.isArray(v)) return v.length === 0 ? "—" : v.map(fmtFieldValue).join(" · ");
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

/** Pairs of (field, before, after) → the fields whose rendered value moved. */
function fieldDiffs(pairs: [string, unknown, unknown][]): FieldDiff[] {
  const out: FieldDiff[] = [];
  for (const [field, a, b] of pairs) {
    const from = fmtFieldValue(a);
    const to = fmtFieldValue(b);
    if (from !== to) out.push({ field, from, to });
  }
  return out;
}

function nodeFieldDiffs(prev: ScryModel, a: Node, b: Node): FieldDiff[] {
  const parentName = (m: ScryModel, id?: string) =>
    id ? m.nodes.find((n) => n.id === id)?.name ?? id : undefined;
  const propsSummary = (n: Node) => (n.properties ?? []).map((p) => p.label);
  return fieldDiffs([
    ["name", a.name, b.name],
    ["kind", a.kind, b.kind],
    ["parent", parentName(prev, a.parentId), parentName(prev, b.parentId) ?? b.parentId],
    ["technology", a.technology, b.technology],
    ["description", a.description, b.description],
    ["visual", !!a.visual, !!b.visual],
    ["properties", propsSummary(a), propsSummary(b)],
  ]);
}

function respFieldDiffs(a: Responsibility, b: Responsibility): FieldDiff[] {
  return fieldDiffs([
    ["statement", a.statement, b.statement],
    ["directives", a.directives, b.directives],
    ["stale", !!a.stale, !!b.stale],
    ["vagrant", !!a.vagrant, !!b.vagrant],
  ]);
}

/** Diff two models into a Recent-changes revision: per-field before → after
 *  on every changed node and claim. Links are included because builds mint
 *  them in bulk. */
function diffRevision(prev: ScryModel, loaded: ScryModel): ChangeItem[] {
  const items: ChangeItem[] = [];
  const prevNodeById = new Map(prev.nodes.map((n) => [n.id, n]));
  const loadedNodeById = new Map(loaded.nodes.map((n) => [n.id, n]));
  const nodeName = (id: string) =>
    loadedNodeById.get(id)?.name || prevNodeById.get(id)?.name || "Untitled";

  for (const n of loaded.nodes) {
    const old = prevNodeById.get(n.id);
    if (!old) {
      items.push({ op: "added", what: "node", label: n.name || "Untitled", nodeId: n.id });
    } else if (nodeFingerprint(old) !== nodeFingerprint(n)) {
      items.push({
        op: "changed",
        what: "node",
        label: n.name || "Untitled",
        nodeId: n.id,
        fields: nodeFieldDiffs(prev, old, n),
      });
    }
  }
  for (const n of prev.nodes)
    if (!loadedNodeById.has(n.id)) {
      // A deleted node has no page of its own — surface it under the surviving
      // parent's history so the deletion is visible somewhere.
      const parentAlive = !!n.parentId && loadedNodeById.has(n.parentId);
      items.push({
        op: "removed",
        what: "node",
        label: n.name || "Untitled",
        context: n.parentId ? nodeName(n.parentId) : undefined,
        nodeId: parentAlive ? n.parentId : undefined,
      });
    }

  const prevResps = new Map<string, { resp: Responsibility; nodeId: string }>();
  for (const n of prev.nodes)
    for (const r of n.responsibilities ?? []) prevResps.set(r.id, { resp: r, nodeId: n.id });
  const loadedRespIds = new Set<string>();
  for (const n of loaded.nodes)
    for (const r of n.responsibilities ?? []) {
      loadedRespIds.add(r.id);
      const old = prevResps.get(r.id);
      const label = r.statement || "Untitled responsibility";
      if (!old) {
        items.push({ op: "added", what: "claim", label, context: n.name, nodeId: n.id });
      } else if (respTruthChanged(old.resp, r)) {
        items.push({
          op: "changed",
          what: "claim",
          label,
          context: n.name,
          nodeId: n.id,
          fields: respFieldDiffs(old.resp, r),
        });
      }
    }
  for (const [id, { resp, nodeId }] of prevResps)
    if (!loadedRespIds.has(id))
      items.push({
        op: "removed",
        what: "claim",
        label: resp.statement || "Untitled responsibility",
        context: nodeName(nodeId),
        // The host may survive the claim's removal — keep the jump if it did.
        nodeId: loadedNodeById.has(nodeId) ? nodeId : undefined,
      });

  const prevLinks = new Set(prev.links.map((l) => l.id));
  const loadedLinks = new Set(loaded.links.map((l) => l.id));
  for (const l of loaded.links)
    if (!prevLinks.has(l.id))
      items.push({
        op: "added",
        what: "link",
        label: `${nodeName(l.src)} → ${nodeName(l.dst)}`,
        nodeId: loadedNodeById.has(l.src) ? l.src : undefined,
      });
  for (const l of prev.links)
    if (!loadedLinks.has(l.id))
      items.push({
        op: "removed",
        what: "link",
        label: `${nodeName(l.src)} → ${nodeName(l.dst)}`,
      });

  return items;
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
  /** The working model — the PLANNED draft the canvas edits (`planned.scry`,
   *  falling back to the committed model when no plan has diverged yet). */
  model: ScryModel | null;
  /** The committed model (`model.scry`) — the diff base. Advances only when the
   *  agent folds implemented work in or extracts from code. */
  committed: ScryModel | null;
  /** Live `diff(committed, planned)` — the plan: how the working draft diverges
   *  from what the code is believed to satisfy. Empty when they're in sync. */
  planDiff: ModelDiff;
  error: string | null;
  recentProjects: string[];
  /** Node ids the agent created since they were last seen — highlighted on the
   *  canvas until the user selects them. */
  newNodeIds: ReadonlySet<string>;
  /** Responsibility ids the agent created since last seen — highlighted until
   *  the user selects the row. */
  newRespIds: ReadonlySet<string>;
  /** Session-local feed of external (agent) writes, newest first — the data
   *  behind the Recent changes special page. */
  changeLog: readonly ChangeRevision[];
  /** Durable committed-model timeline (`.scryer/history.jsonl`), oldest first —
   *  the data behind each node's History tab. Reloaded whenever the model
   *  changes. */
  history: readonly HistoryEvent[];

  /** Open a project. If it has no model, status becomes `needs-model`. */
  openProject: (path: string) => Promise<void>;
  /** Create an empty model in the open project, then load it. */
  createBlankModel: (path: string) => Promise<void>;
  /** Forget the project; UI returns to the picker. */
  closeProject: () => void;
  /** Patch the working (planned) model. Triggers a debounced save to the plan
   *  (suppressed during an agent run). */
  updateModel: (updater: (m: ScryModel) => ScryModel) => void;
  /** While an agent run is active, suppress the canvas write-back so the agent
   *  owns the file — layout stays client-side and is merged back on reload. */
  setAgentRunning: (running: boolean) => void;
  /** Re-read both layers from disk into memory. The agent (MCP) is the
   *  authoritative writer during a build/fill — this both streams its writes in
   *  (plan edits, and the committed model when it folds work in) and loads the
   *  final result, WITHOUT writing our (possibly stale) in-memory model back
   *  over the agent's work. */
  reloadFromDisk: () => Promise<void>;
  /** Clear the "new" highlight for a node (the user selected it). */
  clearNewNode: (id: string) => void;
  /** Clear the "new" highlight for a responsibility (the user selected it). */
  clearNewResp: (id: string) => void;
  /** Clear every unreviewed-change highlight at once (the review page's
   *  "mark all reviewed"). */
  clearAllNew: () => void;
  /** Drop a recent project from localStorage. */
  forgetRecent: (path: string) => void;
}

/// The node-level facts whose change should flag the node for review — name,
/// position, prose, technology, flags, properties. Responsibilities are
/// tracked at row level instead, so a claim edit highlights the claim, not
/// the whole node.
function nodeFingerprint(n: Node): string {
  return JSON.stringify([
    n.name,
    n.kind,
    n.parentId ?? null,
    n.technology ?? null,
    n.description ?? null,
    !!n.visual,
    n.properties ?? [],
  ]);
}

/// Ids the agent introduced OR CHANGED since the previous model, for review
/// highlighting. This is a planning surface: an external write that edits an
/// existing claim (statement, status, directives, flags) must light up the
/// same way a new one does, or agent work passes silently.
function arrivals(prev: ScryModel, loaded: ScryModel) {
  const prevNodeById = new Map(prev.nodes.map((n) => [n.id, n]));
  const newNodes: string[] = [];
  for (const n of loaded.nodes) {
    const old = prevNodeById.get(n.id);
    if (!old || nodeFingerprint(old) !== nodeFingerprint(n)) newNodes.push(n.id);
  }
  const prevResps = new Map<string, Responsibility>();
  for (const n of prev.nodes)
    for (const r of n.responsibilities ?? []) prevResps.set(r.id, r);
  const newResps: { id: string; nodeId: string }[] = [];
  for (const n of loaded.nodes)
    for (const r of n.responsibilities ?? []) {
      const old = prevResps.get(r.id);
      if (!old || respTruthChanged(old, r)) newResps.push({ id: r.id, nodeId: n.id });
    }
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
  const [committed, setCommitted] = useState<ScryModel | null>(null);
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
  const [changeLog, setChangeLog] = useState<ChangeRevision[]>([]);
  const [history, setHistory] = useState<HistoryEvent[]>([]);

  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Exact bytes of our last PLANNED write. A `model-changed` event whose plan
  // content matches this is our own echo and is ignored; anything else (an
  // agent via MCP) is a real external change and gets loaded. Content-based
  // rather than a write counter so inotify event coalescing can't make us miss
  // reloads.
  const lastWrittenRaw = useRef<string | null>(null);
  // Bytes of the committed model we last loaded — the canvas never writes this
  // layer, so it's a pure load-dedup (only the agent advances the committed
  // model, via a fold or extraction).
  const lastCommittedRaw = useRef<string | null>(null);
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
  // User-edit revisions staged inside the setModel updater and flushed by the
  // effect below. Keyed by the prev-model reference so StrictMode's double
  // updater invocation replaces the entry instead of duplicating it.
  const pendingUserRevs = useRef<{ prev: ScryModel; items: ChangeItem[] }[]>([]);

  // Apply a freshly-read PLAN file to in-memory state: flag the ids the agent
  // introduced (and prune removed ones) for review highlighting, remember the
  // bytes so the watcher doesn't echo them, and diff-merge layout so already-
  // placed cards keep their positions while new nodes stay unplaced for
  // autoLayout to seed. Callers dedup on `raw === lastWrittenRaw.current` first.
  const applyLoadedRaw = useCallback((raw: string) => {
    const loaded = JSON.parse(raw) as ScryModel;
    const prevModel = modelStateRef.current;
    if (prevModel) {
      // Journal this external write for the Recent changes page.
      const revItems = diffRevision(prevModel, loaded);
      if (revItems.length > 0) {
        setChangeLog((log) =>
          [{ at: Date.now(), by: "agent" as const, items: revItems }, ...log].slice(
            0,
            CHANGE_LOG_CAP,
          ),
        );
      }
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
    setModel(loaded);
  }, []);

  // Apply a freshly-read COMMITTED model file (the diff base). The canvas never
  // writes this layer; it advances only when the agent folds work in or
  // extracts from code, so there's no review-highlighting to do here — just
  // load it and remember the bytes for load-dedup.
  const applyCommittedRaw = useCallback((raw: string) => {
    lastCommittedRaw.current = raw;
    setCommitted(JSON.parse(raw) as ScryModel);
  }, []);

  // Reload the durable history log. Cheap (append-only JSONL), so we just re-read
  // it wholesale on every model change rather than diffing — the agent op that
  // produced the new event also wrote a `.scry` file the watcher fired on.
  const loadHistory = useCallback(async (ref: string) => {
    try {
      const raw = await invoke<string>("read_history", { refStr: ref });
      setHistory(JSON.parse(raw) as HistoryEvent[]);
    } catch {
      /* no history yet — leave empty */
    }
  }, []);

  // File-watcher subscription — re-read both layers from disk on external
  // writes. The `model-changed` event fires for any `.scry` write under
  // `.scryer/` (the agent edits the plan, and commits land in the committed
  // model), so on each event we reconcile both layers, each deduped against its
  // own last-seen bytes.
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
          const planned = await invoke<string>("read_planned", { refStr: modelRef });
          if (active && planned !== lastWrittenRaw.current) applyLoadedRaw(planned);
        } catch {
          /* transient — ignore */
        }
        try {
          const committedRaw = await invoke<string>("read_model", { refStr: modelRef });
          if (active && committedRaw !== lastCommittedRaw.current) applyCommittedRaw(committedRaw);
        } catch {
          /* transient — ignore */
        }
        if (active) await loadHistory(modelRef);
      });
      if (active) unlisten = off;
      else off();
    })();
    return () => {
      active = false;
      unlisten?.();
    };
  }, [modelRef, applyLoadedRaw, applyCommittedRaw, loadHistory]);

  const openProject = useCallback(async (path: string) => {
    setStatus("loading");
    setError(null);
    setProjectPath(path);
    // The model just loaded is the review baseline — nothing is "new" yet.
    setNewNodeIds(new Set());
    setNewRespIds(new Set());
    setChangeLog([]);
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
        setCommitted(null);
        return;
      }
      const ref = `project:${path}`;
      try {
        // The committed model decides whether a model exists at all; the plan
        // is the working layer (it falls back to the committed model when no
        // draft has diverged yet, so this never throws when the model exists).
        const committedRaw = await invoke<string>("read_model", { refStr: ref });
        const committedModel = JSON.parse(committedRaw) as ScryModel;
        let plannedRaw = committedRaw;
        try {
          plannedRaw = await invoke<string>("read_planned", { refStr: ref });
        } catch {
          /* no plan layer yet — work straight off the committed model */
        }
        const plannedModel = JSON.parse(plannedRaw) as ScryModel;
        // Positions are seeded client-side after card measurement
        // (see autoLayout), not here — we have no DOM to measure against.
        const next = bumpRecent(path);
        writeRecent(next);
        setRecentProjects(next);
        setModelRef(ref);
        setCommitted(committedModel);
        setModel(plannedModel);
        lastCommittedRaw.current = committedRaw;
        lastWrittenRaw.current = plannedRaw;
        await loadHistory(ref);
        setStatus("ready");
      } catch {
        // No model yet — caller can choose to create blank or generate.
        setModelRef(null);
        setModel(null);
        setCommitted(null);
        setStatus("needs-model");
      }
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  }, [loadHistory]);

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
    setCommitted(null);
    setError(null);
    setNewNodeIds(new Set());
    setNewRespIds(new Set());
    setChangeLog([]);
    setHistory([]);
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

  const clearAllNew = useCallback(() => {
    setNewNodeIds(new Set());
    setNewRespIds(new Set());
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
        // No status/baseline bookkeeping here any more: a canvas edit just
        // diverges the plan from the committed model, and `diff(committed,
        // planned)` captures that divergence live — no per-claim baseline
        // snapshot to maintain.
        // Journal the user's commit alongside agent writes — Recent changes
        // shows every editor, attributed. Staged here (idempotently, keyed on
        // `cur`) and flushed to the log by the effect below.
        const revItems = diffRevision(cur, edited);
        if (revItems.length > 0) {
          pendingUserRevs.current = [
            ...pendingUserRevs.current.filter((p) => p.prev !== cur),
            { prev: cur, items: revItems },
          ];
        }
        // Date any responsibility/property whose truth changed in this edit, so
        // the fossilization clock advances for canvas edits. This is the canvas
        // mirror of the agent's Rust-side write stamping; doing it at this single
        // chokepoint covers every edit path (granular, EditModal bulk-commit)
        // and a layout-only edit re-dates nothing.
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
          invoke("write_planned", { refStr: ref, data: serialized }).catch(() => {});
        }, SAVE_DEBOUNCE_MS);
        return next;
      });
    },
    [modelRef],
  );

  // Flush staged user-edit revisions into the journal once the model state
  // has actually advanced. Consecutive intents from one logical edit (Done
  // can fire several) merge into the head revision.
  useEffect(() => {
    const pend = pendingUserRevs.current;
    if (pend.length === 0) return;
    pendingUserRevs.current = [];
    const items = pend.flatMap((p) => p.items);
    const now = Date.now();
    setChangeLog((log) => {
      const head = log[0];
      if (head && head.by === "user" && now - head.at < USER_MERGE_WINDOW_MS) {
        return [{ ...head, at: now, items: [...head.items, ...items] }, ...log.slice(1)];
      }
      return [{ at: now, by: "user" as const, items }, ...log].slice(0, CHANGE_LOG_CAP);
    });
  }, [model]);

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
      const planned = await invoke<string>("read_planned", { refStr: ref });
      if (planned !== lastWrittenRaw.current) applyLoadedRaw(planned);
    } catch {
      /* transient — ignore */
    }
    try {
      const committedRaw = await invoke<string>("read_model", { refStr: ref });
      if (committedRaw !== lastCommittedRaw.current) applyCommittedRaw(committedRaw);
    } catch {
      /* transient — ignore */
    }
    await loadHistory(ref);
  }, [applyLoadedRaw, applyCommittedRaw, loadHistory]);

  // The plan: how the working draft diverges from the committed model. Computed
  // client-side so it tracks every canvas keystroke with no round-trip; the
  // JSON shape mirrors the agent's `get_pending`, so the two never
  // disagree about what's pending.
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
    newNodeIds,
    newRespIds,
    changeLog,
    history,
    openProject,
    createBlankModel,
    closeProject,
    updateModel,
    setAgentRunning,
    reloadFromDisk,
    clearNewNode,
    clearNewResp,
    clearAllNew,
    forgetRecent,
  };
}
