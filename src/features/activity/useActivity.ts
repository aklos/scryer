/**
 * The project's activity: HEAD's commits newest first, a page at a time, and
 * what each changed in the model, plus what is uncommitted on top. Each
 * commit's diff is read once, newest first, one at a time: a commit the
 * backend has not derived before takes seconds, and a burst of them would
 * stall everything else. The list and the uncommitted diff are read again
 * whenever the tree changes.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type Change = "added" | "modified" | "removed";

export interface ClaimChange {
  id: string;
  statement: string;
  /** The wording before, for a reworded claim. */
  was?: string;
  /** `modified` reads as reworded. */
  change: Change;
}

export interface NodeChange {
  id: string;
  name: string;
  kind: string;
  /** Its ancestors' names, outermost first. */
  path: string[];
  change: Change;
  claims: ClaimChange[];
  /** Its other changes, in words. */
  notes?: string[];
}

export interface ModelDiff {
  nodes: NodeChange[];
}

export interface Commit {
  sha: string;
  parent?: string;
  subject: string;
  author: string;
  /** Unix seconds. */
  at: number;
}

/** A diff as it loads. */
export type DiffState = { diff: ModelDiff } | { error: string } | "loading" | undefined;

const PAGE = 20;

/** The claims a diff added, reworded and removed, and the nodes other than
 *  code symbols it changed other than through their claims. */
export function diffTally(d: ModelDiff): { added: number; reworded: number; removed: number; nodes: number } {
  const t = { added: 0, reworded: 0, removed: 0, nodes: 0 };
  for (const n of d.nodes) {
    for (const c of n.claims) t[c.change === "modified" ? "reworded" : c.change]++;
    if (n.kind !== "symbol" && (n.change !== "modified" || (n.notes?.length ?? 0) > 0)) t.nodes++;
  }
  return t;
}

export function useActivity(projectPath: string | null, generation: number) {
  const [commits, setCommits] = useState<Commit[] | null>(null);
  const [more, setMore] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [uncommitted, setUncommitted] = useState<DiffState>(undefined);
  const [diffs, setDiffs] = useState<Record<string, DiffState>>({});
  const shown = useRef(PAGE);
  // The diffs already asked for, so a re-render never asks twice.
  const asked = useRef(new Set<string>());

  const load = useCallback(async () => {
    if (!projectPath) return;
    try {
      const list = await invoke<Commit[]>("get_history", { cwd: projectPath, skip: 0, limit: shown.current });
      setCommits(list);
      setMore(list.length === shown.current);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
    invoke<ModelDiff>("get_uncommitted_diff", { cwd: projectPath })
      .then((diff) => setUncommitted({ diff }))
      .catch((e) => setUncommitted({ error: String(e) }));
  }, [projectPath]);

  useEffect(() => {
    shown.current = PAGE;
    asked.current = new Set();
    setCommits(null);
    setDiffs({});
    setUncommitted(undefined);
  }, [projectPath]);

  useEffect(() => {
    void load();
  }, [load, generation]);

  // Read the diffs not read yet, newest first, one at a time.
  useEffect(() => {
    if (!projectPath || !commits) return;
    let live = true;
    void (async () => {
      for (const c of commits) {
        if (!live) return;
        if (asked.current.has(c.sha)) continue;
        asked.current.add(c.sha);
        setDiffs((d) => ({ ...d, [c.sha]: "loading" }));
        const state: DiffState = await invoke<ModelDiff>("get_commit_diff", { cwd: projectPath, sha: c.sha, parent: c.parent ?? null })
          .then((diff) => ({ diff }))
          .catch((e) => ({ error: String(e) }));
        setDiffs((d) => ({ ...d, [c.sha]: state }));
      }
    })();
    return () => {
      live = false;
    };
  }, [projectPath, commits]);

  const showMore = useCallback(() => {
    shown.current += PAGE;
    void load();
  }, [load]);

  return { commits, more, error, uncommitted, diffs, showMore };
}
