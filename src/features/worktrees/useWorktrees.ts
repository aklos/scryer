/**
 * The project's git worktrees: the cheap list the top bar switches between
 * (`list_worktrees`), and each worktree's work against its base branch for
 * the landing (`get_worktrees`), which derives every changed checkout and
 * can take seconds. A checkout is opened like any project.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CLAIM_LABEL } from "../../entities/model/claimState";

export interface Worktree {
  path: string;
  /** Absent for a detached HEAD. */
  branch?: string | null;
  /** The checkout the app has open. */
  current: boolean;
}

export interface WorktreeClaim {
  respId: string;
  nodeId: string;
  statement: string;
  file: string;
  line: number;
  state: "proven" | "stale" | "failing" | "planned";
  concerns?: string[];
}

export interface WorktreeWork {
  path: string;
  branch?: string;
  current: boolean;
  /** The branch its changes are read against; absent for the main
   *  checkout's own branch, whose changes are what is uncommitted. */
  base?: string;
  ahead: number;
  behind: number;
  files: string[];
  components: { id: string; name: string }[];
  /** Claims its changes reach, with their status in that checkout. */
  claims: WorktreeClaim[];
  ready: boolean;
  error?: string;
}

export interface Worktrees {
  worktrees: WorktreeWork[];
  overlaps: { component: { id: string; name: string }; worktrees: string[] }[];
}

/** Where a worktree stands, in a few words, each in the tone of what it
 *  says: a claim count in its state's hue, anything else plain. */
export type StandingPart = { label: string; tone: "proven" | "stale" | "failing" | "planned" | "none" };

export function standing(w: WorktreeWork): StandingPart[] {
  if (w.error) return [{ label: "could not be read", tone: "failing" }];
  if (w.files.length === 0) {
    // Nothing of its own left: a branch whose work is all in its base.
    if (w.base && w.ahead === 0 && w.behind > 0) return [{ label: `merged · ${w.behind} behind ${w.base}`, tone: "none" }];
    return [{ label: "no changes", tone: "none" }];
  }
  if (w.claims.length === 0) return [{ label: "touches no claims", tone: "none" }];
  if (w.ready) return [{ label: "ready to merge", tone: "proven" }];
  return (["failing", "stale", "planned"] as const)
    .map((state) => ({ state, n: w.claims.filter((c) => c.state === state).length }))
    .filter(({ n }) => n > 0)
    .map(({ state, n }) => ({ label: `${n} ${CLAIM_LABEL[state]}`, tone: state }));
}

/** The project's live worktrees, main checkout first; refreshed on demand. */
export function useWorktreeList(projectPath: string | null) {
  const [list, setList] = useState<Worktree[]>([]);
  const refresh = useCallback(() => {
    if (!projectPath) return setList([]);
    invoke<Worktree[]>("list_worktrees", { cwd: projectPath })
      .then(setList)
      .catch(() => setList([]));
  }, [projectPath]);
  useEffect(refresh, [refresh]);
  return { list, refresh };
}

/** Each worktree's work and the overlaps between them, re-read as sessions
 *  append or reports land. Only the latest request may land. */
export function useWorktrees(projectPath: string | null) {
  const [data, setData] = useState<Worktrees | null>(null);
  const latest = useRef(0);
  const load = useCallback(() => {
    if (!projectPath) return;
    const request = ++latest.current;
    invoke<Worktrees>("get_worktrees", { cwd: projectPath })
      .then((d) => request === latest.current && setData(d))
      .catch(() => request === latest.current && setData(null));
  }, [projectPath]);
  useEffect(() => setData(null), [projectPath]);
  useEffect(() => {
    load();
    const offs = [listen("session-changed", load), listen("test-results-changed", load)];
    return () => offs.forEach((p) => void p.then((f) => f()));
  }, [load]);
  return data;
}
