/**
 * The story the film tells around the derived data: the agent sessions that
 * ran before the film starts, when each verdict was recorded, and where the
 * diagram places the context and containers. A real project has no agent
 * history to show, so this is authored here and laid over either base —
 * demo/derive.mjs over `scryer-mcp export`, fixture.ts over its own tree.
 *
 * Claims are named by their wording and nodes by their names, never by id.
 * Node runs this file directly (derive.mjs), so its imports carry their
 * extension and bring in nothing but types and shapes.ts.
 */

import { resolveNode } from "./shapes.ts";
import type { DerivedClaim, Fixture, Layout, SessionEntry, SessionLog } from "./types.ts";

/** The agent in the film's terminal: deliberately nobody's product. */
export const AGENT_NAME = "your agent";

/** The film's "now", and minutes before it, as unix seconds. */
export const NOW = 1_792_400_000;
const ago = (minutes: number) => NOW - minutes * 60;

/** When every verdict the story does not date was recorded: two days back. */
export const BASE_RECORDED = NOW - 2 * 86_400;

/** Three sessions ran since the user last looked; act 1 starts a fourth. */
export const SESSION_LEDGER = "5e0b7a21-ledger";
export const SESSION_DISPUTES = "a41f9c02-disputes";
export const SESSION_WEBHOOKS = "7be2d510-webhooks";
export const SESSION_REFUNDS = "c3d8e1f7-refunds";

interface Told {
  id: string;
  task: string;
  /** Minutes before now. */
  from: number;
  to: number;
  touch: string[];
  /** Todo tests the session wrote, by claim wording. */
  stubs?: string[];
  /** What its stop reached: a claim by wording, or every proven claim
   *  under a node by name. Proven claims it reached were proven by it — their
   *  verdicts are recorded at its stop. */
  reached: string[];
}

const TOLD: Told[] = [
  {
    id: SESSION_LEDGER,
    task: "build the ledger",
    from: 175,
    to: 128,
    touch: [
      "ledger/src/domain/account.rs",
      "ledger/src/domain/entry.rs",
      "ledger/src/domain/capture.rs",
      "ledger/src/application/store.rs",
      "ledger/src/infrastructure/postgres.rs",
      "ledger/src/infrastructure/bank_file.rs",
      "ledger/src/infrastructure/backfill.rs",
      "ledger/src/post.rs",
      "api-gateway/src/routes/payments.ts",
    ],
    reached: [
      "Ledger",
      "API Gateway",
      "When a cardholder pays by card, captures the payment and credits the merchant",
      "When a merchant refunds a payment, returns the money to the card",
    ],
  },
  {
    id: SESSION_DISPUTES,
    task: "hold disputed funds back from payouts",
    from: 95,
    to: 93,
    touch: ["ledger/src/domain/payout.rs"],
    stubs: ["While a dispute is open, holds the disputed amount back from payouts"],
    reached: [
      "Nets fees out of each payout",
      "While a dispute is open, holds the disputed amount back from payouts",
      "Pays each merchant's settled balance out once a day",
    ],
  },
  {
    id: SESSION_WEBHOOKS,
    task: "retry failed ledger webhooks first",
    from: 14,
    to: 11,
    touch: ["webhooks/delivery/retry.go"],
    stubs: ["When a ledger event fails to send, retries it before newer events"],
    reached: [
      "If a merchant endpoint fails, then retries with backoff for a day",
      "When an endpoint answers 410, stops sending to it",
      "When a ledger event fails to send, retries it before newer events",
    ],
  },
];

/** Code the ledger session wrote that no claim reaches: its new dark file. */
const NEW_DARK = "ledger/src/infrastructure/backfill.rs";

/** Every file the story names, for a base's file map. */
export const STORY_FILES = [...new Set([...TOLD.flatMap((s) => s.touch), NEW_DARK])];

/** Placements of the context and container levels by node name, so the
 *  diagram never lays them out during a render. Inside a styled container the
 *  style's own drawing places the components by layer. */
export const LAYOUT: Layout = {
  // Context
  Cardholder: { x: 120, y: 120 },
  Merchant: { x: 120, y: 420 },
  "Aperture Pay": { x: 520, y: 270 },
  "Acquiring Bank": { x: 940, y: 120 },
  "Card Networks": { x: 940, y: 420 },
  // Containers, inside the system
  Dashboard: { x: 80, y: 80 },
  "API Gateway": { x: 420, y: 80 },
  Fraud: { x: 760, y: 80 },
  Ledger: { x: 420, y: 330 },
  Webhooks: { x: 760, y: 330 },
  "Payments DB": { x: 420, y: 560 },
};

/** The node a file belongs to by the tree's boundaries: the longest
 *  directory pattern holding it. */
function byBoundary(base: Fixture, file: string): string | undefined {
  let best: [string, number] | undefined;
  for (const [id, sources] of Object.entries(base.tree.planned.boundaries ?? {})) {
    for (const { pattern } of sources) {
      const dir = pattern.replace(/\*\*.*$/, "").replace(/\/?$/, "/");
      if (dir !== "/" && file.startsWith(dir) && (!best || dir.length > best[1])) best = [id, dir.length];
    }
  }
  return best?.[0];
}

/** Where a story file sits in the base: the path itself when the base
 *  knows it, else the known file that ends with it, else the path under the
 *  prefix a known sibling directory sits under (`crates/` in a workspace). */
function locate(base: Fixture, file: string): string {
  const known = Object.keys(base.tree.fileNodes);
  if (known.includes(file)) return file;
  const end = known.find((k) => k.endsWith(`/${file}`));
  if (end) return end;
  const dir = file.slice(0, file.lastIndexOf("/") + 1);
  const sibling = known.find((k) => k.includes(`/${dir}`) && k.indexOf(`/${dir}`) + dir.length + 1 === k.lastIndexOf("/") + 1);
  return sibling ? sibling.slice(0, sibling.indexOf(`/${dir}`) + 1) + file : file;
}

/** `base` with the story laid over it: its sessions (the base's own are
 *  dropped — the story is the history), every verdict dated two days back
 *  except those a session proved, the story's files in the file map, its new
 *  dark file, and its placements. Returns what it could not resolve too. */
export function tell(base: Fixture): { fixture: Fixture; unresolved: string[] } {
  const model = base.tree.planned;
  const unresolved: string[] = [];
  const byText = new Map(base.tree.claims.map((c) => [c.description, c]));
  const parent = new Map(model.nodes.map((n) => [n.id, n.parentId]));
  const state = new Map(base.status.claims.map((s) => [s.respId, s.state]));
  const under = (claim: DerivedClaim, node: string) => {
    for (let id: string | undefined = claim.nodeId; id; id = parent.get(id)) if (id === node) return true;
    return false;
  };
  const claim = (text: string) => {
    const c = byText.get(text);
    if (!c) unresolved.push(text);
    return c;
  };
  const reachedBy = (s: Told) =>
    s.reached.flatMap((ref) => {
      if (byText.has(ref)) return [byText.get(ref)!];
      const node = resolveNode(model, ref);
      if (!node) return (unresolved.push(ref), []);
      return base.tree.claims.filter((c) => under(c, node) && state.get(c.id) === "proven");
    });

  const at = (file: string) => locate(base, file);
  const recorded = new Map<string, number>();
  const sessions: SessionLog[] = TOLD.map((s) => {
    const reached = [...new Set(reachedBy(s))];
    for (const c of reached) if (state.get(c.id) === "proven") recorded.set(c.id, ago(s.to));
    const entries: SessionEntry[] = [
      { at: ago(s.from), event: "task", text: s.task },
      ...s.touch.map((file): SessionEntry => ({ at: ago(s.from - 1), event: "touch", file: at(file) })),
      ...(s.stubs ?? []).flatMap((text): SessionEntry[] => {
        const c = claim(text);
        return c ? [{ at: ago(s.from - 1), event: "stub", file: c.file, test: c.junitName }] : [];
      }),
      { at: ago(s.to), event: "reached", ids: reached.map((c) => c.id) },
      { at: ago(s.to), event: "stop" },
    ];
    return { id: s.id, entries };
  });

  const dark = at(NEW_DARK);
  const fileNodes = { ...base.tree.fileNodes };
  for (const file of [...STORY_FILES.map(at), ...base.tree.darkFiles]) {
    const node = fileNodes[file] ?? byBoundary(base, file);
    if (node) fileNodes[file] = node;
    else unresolved.push(file);
  }

  return {
    fixture: {
      ...base,
      tree: {
        ...base.tree,
        fileNodes,
        darkFiles: base.tree.darkFiles.includes(dark) ? base.tree.darkFiles : [...base.tree.darkFiles, dark],
      },
      status: {
        ...base.status,
        claims: base.status.claims.map((s) => ({
          ...s,
          recordedAt: s.state === "planned" ? undefined : (recorded.get(s.respId) ?? BASE_RECORDED),
        })),
      },
      sessions,
      layout: { ...base.layout, ...LAYOUT },
    },
    unresolved: [...new Set(unresolved)],
  };
}
