/**
 * The session log — what one agent session touched, the claims its edits
 * reached, and the plan entries it left unbuilt. Read from the backend's
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
}

export interface SessionView {
  session: string;
  startedAt: number;
  updatedAt: number;
  /** Each edited file and the claims (resp id, statement) its edit affected. */
  touched: { file: string; claims: [string, string][] }[];
  /** Plan entries the session planned and never folded, with any progress note. */
  unfolded?: { key: string; label: string; note?: string }[];
  /** Plan element keys the agent wrote (`resp:…`, `node:…`, …). */
  modelEdits: string[];
}

// --- joins -----------------------------------------------------------------------

/** The top-bar count: what still wants the user's eye — plan entries the
 *  session left unbuilt. */
export function sessionBadge(view: SessionView | null): number {
  return view?.unfolded?.length ?? 0;
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
