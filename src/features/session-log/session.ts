/**
 * The session log — what one agent session was asked, what it did about each
 * ask, and what it touched that nobody asked for. Read from the backend's
 * per-session log (`list_sessions` / `read_session`); this module is the PURE
 * half: the wire types and the joins against the working model the page needs.
 * No React, no IPC — `useSessionLog` fetches, `SessionPage` renders.
 */

import type { ScryModel } from "../../entities/model/viewmodel";

// --- wire types (mirror the Rust SessionView) ------------------------------------

/** One row of `list_sessions`, most recent first. */
export interface SessionSummary {
  session: string;
  /** Unix seconds. */
  updatedAt: number;
  firstPrompt?: string | null;
}

export interface SessionPrompt {
  /** "p1", "p2", … */
  id: string;
  /** The user's prompt, verbatim. */
  text: string;
}

export type AskStatus = "delivered" | "answered" | "descoped" | "open";

/** One thing the user asked for, as the agent broke a prompt down. */
export interface SessionAsk {
  /** "a1", "a2", … */
  id: string;
  /** The prompt id it came from. */
  prompt: string;
  text: string;
  kind: "build" | "answer";
  source?: string;
  /** Responsibility ids the ask is delivered through. */
  claims: string[];
  status: AskStatus;
  /** Why the agent dropped it — set when descoped. */
  reason?: string;
  /** What is still missing — set when open. */
  missing?: string[];
}

export interface SessionView {
  session: string;
  startedAt: number;
  updatedAt: number;
  prompts: SessionPrompt[];
  /** Prompt ids the agent has not broken into asks. */
  unfiled: string[];
  asks: SessionAsk[];
  /** Each edited file and the claims (resp id, statement) its edit affected. */
  touched: { file: string; claims: [string, string][] }[];
  /** Edited files no ask accounts for — the "I didn't ask for that" view. */
  untraced: string[];
  /** Plan entries the session planned and never folded: `[key, label]`. */
  unfolded?: [string, string][];
  /** Plan element keys the agent wrote (`resp:…`, `node:…`, …). */
  modelEdits: string[];
}

// --- joins -----------------------------------------------------------------------

/** One prompt with the asks filed under it. */
export interface PromptEntry {
  prompt: SessionPrompt;
  asks: SessionAsk[];
  /** The agent has not broken this prompt into asks yet. */
  unfiled: boolean;
}

/** Prompts in log order, each with its asks (in log order). An ask citing a
 *  prompt the log does not hold is dropped rather than invented a parent. */
export function groupAsks(view: SessionView): PromptEntry[] {
  const byPrompt = new Map<string, SessionAsk[]>();
  for (const a of view.asks) byPrompt.set(a.prompt, [...(byPrompt.get(a.prompt) ?? []), a]);
  const unfiled = new Set(view.unfiled);
  return view.prompts.map((prompt) => ({
    prompt,
    asks: byPrompt.get(prompt.id) ?? [],
    unfiled: unfiled.has(prompt.id),
  }));
}

/** The top-bar count: what still wants the user's eye — open asks plus files
 *  no ask accounts for. */
export function sessionBadge(view: SessionView | null): number {
  if (!view) return 0;
  return view.asks.filter((a) => a.status === "open").length + view.untraced.length;
}

/** What a plan element key resolves to in the working model, for display and
 *  navigation. `hostId` is the page to open (a node or group); `respId` the
 *  claim to jump to on it. Unresolvable keys keep the raw key as the label. */
export interface ResolvedKey {
  key: string;
  kind: "node" | "group" | "resp" | "link" | "prop" | "other";
  label: string;
  hostKind?: "node" | "group";
  hostId?: string;
  /** The host's name, for "on X" context under a claim. */
  hostName?: string;
  respId?: string;
}

export interface ClaimHost {
  statement: string;
  hostKind: "node" | "group";
  hostId: string;
  hostName: string;
}

/** Every claim in the model by id, with where it lives. */
export function claimIndex(model: ScryModel): Map<string, ClaimHost> {
  const out = new Map<string, ClaimHost>();
  for (const n of model.nodes)
    for (const r of n.responsibilities ?? [])
      out.set(r.id, { statement: r.statement, hostKind: "node", hostId: n.id, hostName: n.name });
  for (const g of model.groups)
    for (const r of g.responsibilities ?? [])
      out.set(r.id, { statement: r.statement, hostKind: "group", hostId: g.id, hostName: g.name });
  return out;
}

export function resolveKey(key: string, model: ScryModel, claims: Map<string, ClaimHost>): ResolvedKey {
  const colon = key.indexOf(":");
  const kind = colon < 0 ? "" : key.slice(0, colon);
  const id = colon < 0 ? key : key.slice(colon + 1);
  switch (kind) {
    case "resp": {
      const c = claims.get(id);
      return c
        ? { key, kind: "resp", label: c.statement, hostKind: c.hostKind, hostId: c.hostId, hostName: c.hostName, respId: id }
        : { key, kind: "resp", label: key };
    }
    case "node": {
      const n = model.nodes.find((x) => x.id === id);
      return n ? { key, kind: "node", label: n.name, hostKind: "node", hostId: n.id } : { key, kind: "node", label: key };
    }
    case "group": {
      const g = model.groups.find((x) => x.id === id);
      return g ? { key, kind: "group", label: g.name, hostKind: "group", hostId: g.id } : { key, kind: "group", label: key };
    }
    case "link": {
      const l = model.links.find((x) => x.id === id);
      const name = (nid: string) => model.nodes.find((n) => n.id === nid)?.name ?? nid;
      return l ? { key, kind: "link", label: `${name(l.src)} → ${name(l.dst)}` } : { key, kind: "link", label: key };
    }
    case "prop": {
      // prop:<owner>:<label> — the label may itself hold colons.
      const sep = id.indexOf(":");
      const owner = sep < 0 ? "" : id.slice(0, sep);
      const n = model.nodes.find((x) => x.id === owner);
      return n
        ? { key, kind: "prop", label: `${n.name}.${id.slice(sep + 1)}`, hostKind: "node", hostId: n.id }
        : { key, kind: "prop", label: key };
    }
    default:
      return { key, kind: "other", label: key };
  }
}
