/**
 * The project's agent sessions as they work (`get_landing`): each session's
 * steps in order, what its edits changed the state of, and whether it is
 * working now. Re-read as sessions append and reports land; the Sessions
 * page lists them and following one drives the wiki and the map. A read
 * derives every worktree's tree and can take seconds.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ClaimState = "proven" | "stale" | "failing" | "planned";

export interface SessionClaim {
  respId: string;
  nodeId: string;
  statement: string;
  file: string;
  line: number;
  state: ClaimState;
  /** Concern tags, shown at the row's end. */
  concerns?: string[];
}

/** One thing a session did, oldest first in its list. */
export type SessionStep =
  | { kind: "edit"; at: number; file: string; nodeId?: string; edits: number }
  | { kind: "todo"; at: number; claim: SessionClaim }
  | { kind: "verdict"; at: number; claim: SessionClaim }
  | { kind: "probe"; at: number; open: boolean }
  | { kind: "stop"; at: number };

export interface LoggedSession {
  session: string;
  title: string;
  /** The title is the task the agent named when it oriented. */
  titledByTask?: boolean;
  startedAt: number;
  endedAt: number;
  newlyProven: SessionClaim[];
  newlyPlanned: SessionClaim[];
  stillPlanned: SessionClaim[];
  newlyStaleOrFailing: SessionClaim[];
  survivingMutants: number;
  newDarkFiles: string[];
  components: string[];
  /** The worktree the session worked in, when the project has several. */
  worktree?: string;
  /** Working now. */
  live?: boolean;
  steps?: SessionStep[];
}

export interface SessionWorktree {
  path: string;
  branch?: string;
  current: boolean;
  sessions: number;
}

export interface Sessions {
  since: number;
  sessions: LoggedSession[];
  shared: { nodeId: string; sessions: string[] }[];
  /** The open checkout's branch. */
  branch?: string;
  /** The project's live worktrees; absent for a single checkout. */
  worktrees?: SessionWorktree[];
}

/** How far back the Sessions page reaches unless asked for all. */
export const RECENT_SECS = 7 * 24 * 3600;

export const when = (at: number) =>
  new Date(at * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

/** A session's name: the task its agent oriented on, in quotes as the
 *  agent's words; else "session in" the first component it touched; else
 *  "session on" its branch and start time. */
export function sessionTitle(s: Pick<LoggedSession, "title" | "titledByTask" | "startedAt">, branch: string): string {
  if (s.title && s.titledByTask) return `“${s.title}”`;
  if (s.title) return `session in ${s.title}`;
  return `session on ${branch} · ${when(s.startedAt)}`;
}

/** The claims a session moved, as a short phrase per kind: proven, now
 *  failing, todo it wrote; empty when it moved none. */
export function sessionCounts(s: LoggedSession): string[] {
  const failing = s.newlyStaleOrFailing.filter((c) => c.state === "failing").length;
  return [
    s.newlyProven.length > 0 && `${s.newlyProven.length} proven`,
    failing > 0 && `${failing} failing`,
    s.newlyPlanned.length > 0 && `${s.newlyPlanned.length} todo`,
  ].filter((x): x is string => typeof x === "string");
}

/** Where a followed session is working: the node of its latest claim step
 *  (a todo it wrote, a verdict it got) or edit in a component; null before
 *  it has one. */
export function followTarget(s: Pick<LoggedSession, "steps">): string | null {
  const steps = s.steps ?? [];
  for (let i = steps.length - 1; i >= 0; i--) {
    const step = steps[i];
    if (step.kind === "edit" && step.nodeId) return step.nodeId;
    if (step.kind === "todo" || step.kind === "verdict") return step.claim.nodeId;
  }
  return null;
}

/** A step as one line of the feed, naming components by `name`. */
export function stepLine(step: SessionStep, name: (id: string) => string): string {
  switch (step.kind) {
    case "edit":
      return `edited ${step.file}${step.nodeId ? ` in ${name(step.nodeId)}` : ""}${step.edits > 1 ? ` ×${step.edits}` : ""}`;
    case "todo":
      return `planned: ${step.claim.statement}`;
    case "verdict":
      return `${step.claim.state === "proven" ? "proved" : "failed"}: ${step.claim.statement}`;
    case "probe":
      return step.open ? "opened a probe" : "closed its probe";
    case "stop":
      return "finished a turn";
  }
}

/** The sessions active since `since` (unix seconds; 0 for all), live ones
 *  first, then newest first. Only the latest request may land, so a slow
 *  earlier read never overwrites a newer answer. Off while `projectPath` is
 *  null. */
export function useSessions(projectPath: string | null, since: number) {
  const [data, setData] = useState<Sessions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(0);
  const load = useCallback(() => {
    if (!projectPath) return;
    const request = ++latest.current;
    invoke<Sessions>("get_landing", { cwd: projectPath, since })
      .then((d) => {
        if (request !== latest.current) return;
        setData({ ...d, sessions: liveFirst(d.sessions) });
        setError(null);
      })
      .catch((e) => {
        if (request === latest.current) setError(String(e));
      });
  }, [projectPath, since]);
  // A new window shows nothing until its own answer arrives, never the
  // previous window's sessions.
  useEffect(() => setData(null), [projectPath, since]);
  useEffect(() => {
    load();
    const offs = [listen("session-changed", load), listen("test-results-changed", load)];
    return () => offs.forEach((p) => void p.then((f) => f()));
  }, [load]);
  return { data, error };
}

/** Live sessions first, each group newest first. */
export function liveFirst(sessions: LoggedSession[]): LoggedSession[] {
  return [...sessions].sort((a, b) => Number(!!b.live) - Number(!!a.live) || b.endedAt - a.endedAt);
}
