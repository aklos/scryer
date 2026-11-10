/**
 * Following an agent session, as pure rules: where the session is working (the node of its latest claim step or
 * edit in a component), when the view may move there (once that node has held for the settle time, so a burst of
 * edits across files never makes the view twitch), what pauses it (going elsewhere yourself, accepting an agent's
 * show) and which sessions the app offers to follow.
 *
 * Follow is mechanical and nothing else: an agent's `show` is a separate, explicit act that pauses any follow and
 * takes the developer where the agent wants to talk. A paused follow resumes only when the developer says so.
 */

import type { LoggedSession, SessionStep } from "./useSessions";

/** How long the session's latest node must hold before the view goes there, in ms. */
export const SETTLE_MS = 2500;

/** Where a followed session is working: the node of its latest claim step (a todo it wrote, a verdict it got) or
 *  edit in a component; null before it has one. */
export function followTarget(s: Pick<LoggedSession, "steps">): string | null {
  const steps: SessionStep[] = s.steps ?? [];
  for (let i = steps.length - 1; i >= 0; i--) {
    const step = steps[i];
    if (step.kind === "edit" && step.nodeId) return step.nodeId;
    if (step.kind === "todo" || step.kind === "verdict") return step.claim.nodeId;
  }
  return null;
}

/** The node the view should go to now: the target, once it has held `SETTLE_MS` since `since` and the view isn't
 *  already there; else null. */
export function dueMove({
  target,
  since,
  wentTo,
  now,
}: {
  target: string | null;
  /** When the target last changed, in ms. */
  since: number;
  /** Where the follow last took the view. */
  wentTo: string | null;
  now: number;
}): string | null {
  if (!target || target === wentTo) return null;
  return now - since >= SETTLE_MS ? target : null;
}

export type FollowState = { session: string; paused: boolean } | null;

export type FollowAction =
  | { type: "follow"; session: string }
  /** The developer went elsewhere themself. */
  | { type: "away" }
  /** The developer accepted an agent's show. */
  | { type: "show" }
  | { type: "resume" }
  | { type: "stop" }
  /** The followed session stopped being live. */
  | { type: "ended" };

export function followReducer(s: FollowState, a: FollowAction): FollowState {
  switch (a.type) {
    case "follow":
      return { session: a.session, paused: false };
    case "away":
    case "show":
      return s && { ...s, paused: true };
    case "resume":
      return s && { ...s, paused: false };
    case "stop":
    case "ended":
      return null;
  }
}

/** One session as `get_session_brief` reads it from its log. */
export interface SessionBrief {
  session: string;
  task?: string;
  live: boolean;
  touched: number;
}

/** Whether to offer following `b`: it is live and has begun work (oriented on a task or edited a file), and it
 *  was neither offered before nor is it the one followed. */
export function offerFor(b: SessionBrief, { offered, follow }: { offered: ReadonlySet<string>; follow: FollowState }): boolean {
  if (!b.live || (!b.task && b.touched === 0)) return false;
  return !offered.has(b.session) && follow?.session !== b.session;
}

/** A step as one line, naming components by `name`: what the followed session did last. */
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
