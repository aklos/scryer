/**
 * The in-session inbox — one live stream of cards, each an item that needs the
 * developer's verdict, merged from every source the app already watches:
 *
 *   - amendments / additions after sign-off (`vagrantOrigin`, forward-vagrancy)
 *   - code-discovered vagrant claims and properties (drift)
 *   - stale claims, with drift's proposed reword when it has one
 *   - probe survivors (a deliberate break the attached test did not catch)
 *   - failing / errored test verdicts
 *   - contract-level plan entries (a reworded claim on a container / system, a
 *     new or repointed link crossing containers)
 *   - refused folds (`mark_implemented` declined a claim and said why)
 *   - close-gate items (a session touched an anchored span and left it
 *     unreconciled)
 *
 * PURE: no React, no IPC. `useInbox` gathers the sources; `InboxPage` renders.
 * Untested claims are deliberately NOT cards — they are a standing list (Needs
 * review keeps them), not a verdict.
 *
 * Ordering is by risk tier, then recency. Card ids are stable across rebuilds
 * (derived from the source key, never from position) so "seen" and "dismissed"
 * survive a re-render — and a card whose source changes (a new probe result, a
 * different reword) gets a NEW id and resurfaces.
 */

import type { ClaimProbeStatus, ClaimTestStatus } from "./health";
import { probeMark, testLaneGlyph } from "./health";
import { elementKey } from "./ledger";
import type { Change, ModelDiff } from "./planDiff";
import type { Group, Node, Responsibility, ScryModel, SourceLocation } from "./viewmodel";
import { effectiveSourceMap, effectiveTestMap } from "./viewmodel";

// --- input types -----------------------------------------------------------------

/** One claim `mark_implemented` declined to fold, and why. Mirrors Rust
 *  `scryer_core::refusals::Refusal`; read via `read_fold_refusals`. The backend
 *  removes it when the claim later folds or leaves the plan. */
export interface Refusal {
  respId: string;
  /** The node or group the claim sits on. */
  hostId: string;
  kind: "no-test" | "no-verdict" | "stale" | "failing" | "amendment" | "addition";
  /** The missing fact in the fold's own words. */
  reason: string;
  /** Test files whose run would clear the refusal, when any. */
  run?: string[];
  /** Unix seconds. */
  at: number;
}

/** One claim a session's close gate flagged: it touched the claim's anchored
 *  span and left it unreconciled. Flattened from the `hook-close-gate` event
 *  (one per `needsReconcile[].claims[]`). */
export interface CloseGateItem {
  session: string;
  file: string;
  /** The anchor key — a responsibility id, or a node id for a data-shape
   *  declaration anchor. */
  id: string;
  /** Host node/group NAME (the gate resolves names, not ids). */
  host?: string;
  symbol?: string;
  /** Anchor state: changed / broken / fileMissing / unreconciled. */
  state?: string;
  statement?: string;
  /** Unix seconds the gate fired. */
  at: number;
}

export interface InboxInput {
  model: ScryModel;
  committed: ScryModel | null;
  planDiff: ModelDiff;
  verdicts: Record<string, ClaimTestStatus>;
  probes: Record<string, ClaimProbeStatus>;
  refusals: Refusal[];
  closeGate: CloseGateItem[];
  /** Element key → change id. Defaults to `model.changeMap`. */
  changeMap?: Record<string, string>;
  /** Unix seconds "now" — injectable for tests; defaults to the clock. */
  now?: number;
}

// --- card types ------------------------------------------------------------------

/** Risk tiers, highest first. Within a tier, newest first. */
export type InboxTier =
  | "contract"
  | "concern"
  | "survivor"
  | "amendment"
  | "vagrant"
  | "stale"
  | "failing"
  | "refused"
  | "close-gate";

export const TIER_ORDER: readonly InboxTier[] = [
  "contract",
  "concern",
  "survivor",
  "amendment",
  "vagrant",
  "stale",
  "failing",
  "refused",
  "close-gate",
];

/** What produced the card — finer than the tier (a concern-promoted card keeps
 *  its source kind so the page can still render the right body and actions). */
export type InboxKind =
  | "amendment"
  | "addition"
  | "vagrant-claim"
  | "vagrant-property"
  | "stale"
  | "survivor"
  | "failing"
  | "contract-reword"
  | "contract-link"
  | "refused"
  | "close-gate";

export type InboxActionKind =
  | "adopt"
  | "reject"
  | "reword"
  | "accept-proposal"
  | "reimplement"
  | "drop"
  | "open-test"
  | "dismiss"
  | "approve"
  | "holds"
  | "flag"
  | "open";

export interface InboxAction {
  kind: InboxActionKind;
  label: string;
  title?: string;
}

export interface InboxEvidence {
  /** Anchored source spans (working view) — the peek links. */
  anchors: SourceLocation[];
  /** Attached tests. */
  tests: SourceLocation[];
  verdict?: ClaimTestStatus;
  probe?: ClaimProbeStatus;
  /** The one glyph the test lane shows (see `testLaneGlyph`). */
  testLane: ReturnType<typeof testLaneGlyph>;
  probeMark: ReturnType<typeof probeMark>;
  /** Survivor cards: the mutations that went uncaught. */
  survivors?: string[];
  /** Refused-fold cards: the refusal itself. */
  refusal?: Refusal;
  /** Close-gate cards: where the touch landed. */
  closeGate?: CloseGateItem;
  /** Contract cards: names of nodes that depend on the changed one (link
   *  sources pointing at it) — the blast radius. */
  dependents?: string[];
}

export interface InboxCard {
  /** Stable across rebuilds — the seen/dismissed key. */
  id: string;
  tier: InboxTier;
  kind: InboxKind;
  respId?: string;
  /** Property cards: the field label (properties have no id). */
  propLabel?: string;
  /** Host element id — a node, or a group for group-held claims. */
  nodeId: string;
  nodeName: string;
  hostKind: "node" | "group";
  /** Progressive-disclosure breadcrumb: the component first, the symbol under
   *  it when the host is a symbol. */
  componentName: string;
  symbolName?: string;
  /** The claim's concern slug, when tagged. */
  concern?: string;
  changeId?: string;
  /** Unix seconds — the recency key. */
  at: number;
  title: string;
  /** The claim (or field) text, raw markup. */
  statement?: string;
  before?: string;
  after?: string;
  evidence: InboxEvidence;
  actions: InboxAction[];
}

// --- helpers ---------------------------------------------------------------------

interface Host {
  kind: "node" | "group";
  id: string;
  name: string;
  node?: Node;
  group?: Group;
}

function tierRank(t: InboxTier): number {
  return TIER_ORDER.indexOf(t);
}

/** FNV-1a 32 as hex — cheap content stamp for ids that must change when the
 *  card's substance does (a new reword, a fresh probe result). */
export function stamp(s: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16);
}

function componentOf(host: Host, byId: Map<string, Node>): { component: string; symbol?: string } {
  const node = host.node;
  if (node && node.kind === "symbol") {
    const parent = node.parentId ? byId.get(node.parentId) : undefined;
    return { component: parent?.name ?? node.name, symbol: node.name };
  }
  return { component: host.name };
}

function evidenceFor(
  respId: string | undefined,
  sourceMap: Record<string, SourceLocation[]>,
  testMap: Record<string, SourceLocation[]>,
  verdicts: Record<string, ClaimTestStatus>,
  probes: Record<string, ClaimProbeStatus>,
): InboxEvidence {
  const verdict = respId ? verdicts[respId] : undefined;
  const probe = respId ? probes[respId] : undefined;
  return {
    anchors: respId ? (sourceMap[respId] ?? []) : [],
    tests: respId ? (testMap[respId] ?? []) : [],
    verdict,
    probe,
    testLane: testLaneGlyph(verdict, probe),
    probeMark: probeMark(probe),
  };
}

const A = (kind: InboxActionKind, label: string, title?: string): InboxAction => ({ kind, label, title });

/** The nearest container-or-above ancestor of a node (itself when it is one),
 *  or null when it sits under none (a top-level component, a person). */
function containerOf(id: string, byId: Map<string, Node>): string | null {
  const seen = new Set<string>();
  let cur: Node | undefined = byId.get(id);
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    if (cur.kind === "container" || cur.kind === "system") return cur.id;
    cur = cur.parentId ? byId.get(cur.parentId) : undefined;
  }
  return null;
}

// --- the build -------------------------------------------------------------------

export function buildInboxCards(input: InboxInput): InboxCard[] {
  const { model, committed, planDiff, verdicts, probes, refusals, closeGate } = input;
  const changeMap = input.changeMap ?? model.changeMap ?? {};
  const now = input.now ?? Math.floor(Date.now() / 1000);
  const sourceMap = effectiveSourceMap(committed, model);
  const testMap = effectiveTestMap(committed, model);
  const byId = new Map(model.nodes.map((n) => [n.id, n] as const));
  const evidence = (respId?: string) => evidenceFor(respId, sourceMap, testMap, verdicts, probes);

  // Every claim with its host, both node- and group-held.
  const claims: { host: Host; resp: Responsibility }[] = [];
  const hostOfResp = new Map<string, Host>();
  for (const n of model.nodes) {
    const host: Host = { kind: "node", id: n.id, name: n.name, node: n };
    for (const r of n.responsibilities ?? []) {
      claims.push({ host, resp: r });
      hostOfResp.set(r.id, host);
    }
  }
  for (const g of model.groups) {
    const host: Host = { kind: "group", id: g.id, name: g.name, group: g };
    for (const r of g.responsibilities ?? []) {
      claims.push({ host, resp: r });
      hostOfResp.set(r.id, host);
    }
  }
  const staleNodeIds = new Set(model.nodes.filter((n) => n.stale).map((n) => n.id));

  const cards: InboxCard[] = [];
  const base = (host: Host, resp: Responsibility | undefined) => {
    const crumb = componentOf(host, byId);
    return {
      respId: resp?.id,
      nodeId: host.id,
      nodeName: host.name,
      hostKind: host.kind,
      componentName: crumb.component,
      symbolName: crumb.symbol,
      concern: resp?.concern,
      changeId: resp ? changeMap[elementKey("responsibility", host.id, resp.id)] : undefined,
      statement: resp?.statement,
    };
  };

  for (const { host, resp } of claims) {
    const at = resp.lastTouchedAt ?? 0;

    // 1) Amendments / additions after sign-off — the agent changed the
    //    signed-off plan; the developer's own intent is in question.
    if (resp.vagrantOrigin) {
      const addition = resp.vagrantOrigin === "addition";
      cards.push({
        ...base(host, resp),
        id: `${resp.vagrantOrigin}:${resp.id}:${stamp(resp.statement)}`,
        tier: "amendment",
        kind: resp.vagrantOrigin,
        at,
        title: addition ? "Added after sign-off" : "Reworded after sign-off",
        before: addition ? undefined : resp.approvedStatement,
        after: resp.statement,
        evidence: evidence(resp.id),
        actions: [
          A("adopt", "Adopt", addition ? "The added claim becomes intent — folds once built and verified" : "The amended text becomes the intent — folds once built and verified"),
          A("reject", "Reject", addition ? "Remove the claim the plan never approved" : "Restore the approved text; the work stays open"),
          A("reword", "Reword", "Replace it with your own wording"),
        ],
      });
      continue;
    }

    // 2) Code-discovered vagrant claims.
    if (resp.vagrant) {
      cards.push({
        ...base(host, resp),
        id: `vagrant-claim:${resp.id}`,
        tier: "vagrant",
        kind: "vagrant-claim",
        at,
        title: "In the code, not in the model",
        evidence: evidence(resp.id),
        actions: [
          A("adopt", "Adopt", "Accept this discovered behaviour into the contract"),
          A("reject", "Reject", "Mark the code for deletion — this behaviour is not wanted"),
          A("reword", "Reword", "Adopt it under your own wording"),
        ],
      });
      continue;
    }

    // 3) Stale claims (a whole stale node is verdicted as a subtree elsewhere).
    if (resp.stale && !(host.kind === "node" && staleNodeIds.has(host.id))) {
      const proposal = resp.staleProposal;
      cards.push({
        ...base(host, resp),
        id: `stale:${resp.id}:${stamp(proposal ?? "")}`,
        tier: "stale",
        kind: "stale",
        at,
        title: proposal ? "Code diverged — drift proposes a reword" : "Code no longer does this",
        before: proposal ? resp.statement : undefined,
        after: proposal,
        evidence: evidence(resp.id),
        actions: [
          ...(proposal ? [A("accept-proposal", "Accept reword", "The code changed what it does — accept drift's wording. No rebuild.")] : []),
          A("reimplement", "Keep claim, rebuild", "Keep this claim as written and rebuild the behaviour in code"),
          A("drop", "Drop", "The behaviour was removed on purpose — drop the claim"),
        ],
      });
    }

    // 4) Probe survivors — the test stayed green while the code was broken.
    const probe = probes[resp.id];
    if (probe && !probe.stale && probe.survived > 0) {
      cards.push({
        ...base(host, resp),
        id: `survivor:${resp.id}:${probe.recordedAt}`,
        tier: "survivor",
        kind: "survivor",
        at: probe.recordedAt,
        title: `${probe.survived} of ${probe.probes} deliberate break${probe.probes === 1 ? "" : "s"} went uncaught`,
        evidence: { ...evidence(resp.id), survivors: probe.survivors },
        actions: [
          A("open-test", "Open test", "Open the claim and its attached test"),
          A("dismiss", "Dismiss", "Hide this card — the next probe result brings it back if it still misses"),
        ],
      });
    }

    // 5) Failing / errored verdicts (a stale verdict is outdated, not an alarm).
    const verdict = verdicts[resp.id];
    if (verdict && !verdict.stale && (verdict.outcome === "failed" || verdict.outcome === "errored")) {
      cards.push({
        ...base(host, resp),
        id: `failing:${resp.id}:${verdict.recordedAt}`,
        tier: "failing",
        kind: "failing",
        at: verdict.recordedAt,
        title: verdict.outcome === "errored" ? "Attached test errored" : "Attached test failing",
        evidence: evidence(resp.id),
        actions: [A("open-test", "Open test", "Open the claim and its attached test")],
      });
    }
  }

  // 2b) Code-discovered vagrant properties — the field-level twin.
  for (const n of model.nodes) {
    for (const p of n.properties ?? []) {
      if (!p.vagrant) continue;
      const host: Host = { kind: "node", id: n.id, name: n.name, node: n };
      cards.push({
        ...base(host, undefined),
        id: `vagrant-property:${n.id}:${p.label}`,
        tier: "vagrant",
        kind: "vagrant-property",
        propLabel: p.label,
        changeId: changeMap[elementKey("property", n.id, p.label)],
        at: p.lastTouchedAt ?? 0,
        title: "Field in the code, not in the model",
        statement: p.description ? `${p.label} — ${p.description}` : p.label,
        evidence: evidence(undefined),
        actions: [
          A("adopt", "Adopt", "The field already exists — fold it into the model"),
          A("reject", "Reject", "Mark the field for deletion"),
        ],
      });
    }
  }

  // 6) Contract-level plan entries.
  const links = [...(committed?.links ?? []), ...model.links];
  const linkById = new Map(links.map((l) => [l.id, l] as const));
  const dependentsOf = (nodeId: string): string[] => {
    const out = new Set<string>();
    for (const l of model.links) if (l.dst === nodeId && l.src !== nodeId) out.add(byId.get(l.src)?.name ?? l.src);
    return [...out];
  };
  for (const ec of planDiff.changes) {
    if (ec.kind === "responsibility" && ec.ownerId) {
      const owner = byId.get(ec.ownerId);
      if (!owner || (owner.kind !== "container" && owner.kind !== "system")) continue;
      const resp = (owner.responsibilities ?? []).find((r) => r.id === ec.id);
      if (!resp || resp.vagrant) continue; // vagrant/amended claims have their own card
      const reword = ec.changes.find(
        (c): c is Extract<Change, { type: "reworded" }> => c.type === "reworded" && c.field === "statement",
      );
      if (!reword) continue;
      const host: Host = { kind: "node", id: owner.id, name: owner.name, node: owner };
      cards.push({
        ...base(host, resp),
        id: `contract-reword:${resp.id}:${stamp(reword.to)}`,
        tier: "contract",
        kind: "contract-reword",
        at: resp.lastTouchedAt ?? 0,
        title: `${owner.kind === "system" ? "System" : "Container"}-level claim reworded in the plan`,
        before: reword.from,
        after: reword.to,
        evidence: { ...evidence(resp.id), dependents: dependentsOf(owner.id) },
        actions: [
          A("approve", "Approve", "Looks right — clear it from the inbox"),
          A("open", "Open", "Open the node page"),
        ],
      });
    } else if (ec.kind === "link") {
      const added = ec.changes.some((c) => c.type === "added");
      const repointed = ec.changes.find(
        (c): c is Extract<Change, { type: "repointed" }> => c.type === "repointed",
      );
      if (!added && !repointed) continue;
      const link = linkById.get(ec.id);
      if (!link) continue;
      const src = repointed?.srcTo ?? link.src;
      const dst = repointed?.dstTo ?? link.dst;
      if (containerOf(src, byId) === containerOf(dst, byId)) continue;
      const srcNode = byId.get(src);
      if (!srcNode) continue;
      const host: Host = { kind: "node", id: srcNode.id, name: srcNode.name, node: srcNode };
      const dstName = byId.get(dst)?.name ?? dst;
      cards.push({
        ...base(host, undefined),
        id: `contract-link:${ec.id}:${stamp(`${src}>${dst}:${link.label}`)}`,
        tier: "contract",
        kind: "contract-link",
        changeId: changeMap[elementKey("link", undefined, ec.id)],
        at: now,
        title: added ? "New cross-container link in the plan" : "Link repointed across containers",
        statement: `${srcNode.name} → ${dstName}${link.label ? ` — ${link.label}` : ""}`,
        before: repointed ? `${byId.get(repointed.srcFrom)?.name ?? repointed.srcFrom} → ${byId.get(repointed.dstFrom)?.name ?? repointed.dstFrom}` : undefined,
        after: repointed ? `${srcNode.name} → ${dstName}` : undefined,
        evidence: { ...evidence(undefined), dependents: dependentsOf(dst) },
        actions: [
          A("approve", "Approve", "Looks right — clear it from the inbox"),
          A("open", "Open", "Open the node page"),
        ],
      });
    }
  }

  // 7) Refused folds — informational; the backend clears them on a later fold.
  for (const ref of refusals) {
    const host = hostOfResp.get(ref.respId);
    const resp = host
      ? ((host.node?.responsibilities ?? host.group?.responsibilities ?? []).find((r) => r.id === ref.respId))
      : undefined;
    // An amended claim already has its own card; the refusal restates it.
    if (resp?.vagrantOrigin && (ref.kind === "amendment" || ref.kind === "addition")) continue;
    const fallback: Host = host ?? {
      kind: "node",
      id: ref.hostId,
      name: byId.get(ref.hostId)?.name ?? model.groups.find((g) => g.id === ref.hostId)?.name ?? ref.hostId,
      node: byId.get(ref.hostId),
    };
    cards.push({
      ...base(fallback, resp),
      respId: ref.respId,
      id: `refused:${ref.respId}:${ref.at}`,
      tier: "refused",
      kind: "refused",
      at: ref.at,
      title: `Fold refused — ${ref.reason}`,
      evidence: { ...evidence(ref.respId), refusal: ref },
      actions: [A("open", "Open", "Open the claim on its page")],
    });
  }

  // 8) Close-gate items — a session touched an anchored span and moved on.
  for (const item of closeGate) {
    const host =
      hostOfResp.get(item.id) ??
      (byId.has(item.id)
        ? ({ kind: "node", id: item.id, name: byId.get(item.id)!.name, node: byId.get(item.id) } as Host)
        : undefined) ??
      (() => {
        const n = model.nodes.find((x) => x.name === item.host);
        return n ? ({ kind: "node", id: n.id, name: n.name, node: n } as Host) : undefined;
      })();
    if (!host) continue;
    const resp = (host.node?.responsibilities ?? host.group?.responsibilities ?? []).find((r) => r.id === item.id);
    cards.push({
      ...base(host, resp),
      respId: resp?.id,
      id: `close-gate:${item.session}:${item.file}:${item.id}`,
      tier: "close-gate",
      kind: "close-gate",
      at: item.at,
      title: `Session touched ${item.symbol ? `\`${item.symbol}\`` : "the span"} in ${item.file}${item.state ? ` (${item.state})` : ""}`,
      statement: resp?.statement ?? item.statement,
      evidence: { ...evidence(resp?.id), closeGate: item },
      actions: [
        A("holds", "Still holds", "The claim still describes the code — clear it"),
        ...(resp ? [A("reword", "Reword", "The code changed what it does — reword the claim")] : []),
        A("flag", "Flag", "Open the node page to look closer"),
      ],
    });
  }

  // Concern promotion: a claim tagged with a cross-cutting concern is riskier
  // than its source alone says — lift anything below "concern" up to it.
  const concernRank = tierRank("concern");
  for (const c of cards) {
    if (c.concern && tierRank(c.tier) > concernRank) c.tier = "concern";
  }

  cards.sort((a, b) => tierRank(a.tier) - tierRank(b.tier) || b.at - a.at || a.id.localeCompare(b.id));
  return cards;
}

/** How many cards the developer has not seen. */
export function inboxUnread(cards: readonly InboxCard[], seenIds: ReadonlySet<string>): number {
  let n = 0;
  for (const c of cards) if (!seenIds.has(c.id)) n++;
  return n;
}

/** The stream pinned to one change; null/undefined = everything. */
export function filterByChange(cards: readonly InboxCard[], changeId: string | null | undefined): InboxCard[] {
  if (!changeId) return [...cards];
  return cards.filter((c) => c.changeId === changeId);
}

/** Flatten a `hook-close-gate` event payload into per-claim items. */
export function closeGateItems(payload: {
  session: string;
  needsReconcile?: { file: string; claims?: { id: string; host?: string; symbol?: string; state?: string; statement?: string }[] }[];
}, at: number): CloseGateItem[] {
  const out: CloseGateItem[] = [];
  for (const f of payload.needsReconcile ?? [])
    for (const c of f.claims ?? [])
      out.push({ session: payload.session, file: f.file, id: c.id, host: c.host, symbol: c.symbol, state: c.state, statement: c.statement, at });
  return out;
}

/** What a keypress on the inbox stream means, resolved against the focused
 *  card — the pure half of the page's keyboard handling so it can be tested
 *  without a DOM. `j`/`k` move the focus (clamped to the list), `Enter` opens
 *  the focused card's node page, and `a`/`r`/`e` run the card's adopt /
 *  reject / reword action when it carries one; anything else, or a card that
 *  lacks the action, is `null` (the key is left to the browser). */
export type InboxKeyAction =
  | { type: "focus"; index: number }
  | { type: "open"; card: InboxCard }
  | { type: "run"; card: InboxCard; action: InboxAction };

export function inboxKeyAction(
  key: string,
  cards: readonly InboxCard[],
  focus: number,
): InboxKeyAction | null {
  const card = cards[focus];
  switch (key) {
    case "j":
      return { type: "focus", index: Math.min(cards.length - 1, focus + 1) };
    case "k":
      return { type: "focus", index: Math.max(0, focus - 1) };
    case "Enter":
      return card ? { type: "open", card } : null;
    case "a":
    case "r":
    case "e": {
      if (!card) return null;
      const kind = key === "a" ? "adopt" : key === "r" ? "reject" : "reword";
      const action = card.actions.find((x) => x.kind === kind);
      return action ? { type: "run", card, action } : null;
    }
    default:
      return null;
  }
}
