/**
 * Trailer fixtures — a hand-authored fictional payments platform ("Aperture
 * Pay") used to drive the demo scenes. These are plain data objects shaped to
 * the real app types (`ScryModel`, `ModelHealthReport`, `ModelDiff`), so the
 * lifted components render exactly as they would in the product, just on
 * curated state we control.
 *
 * The names are chosen to read clearly at trailer speed: a context tier
 * (people + the platform + external systems) over a container tier (the
 * services inside the platform), with links the diagram can lay out.
 */

import type { ScryModel, Node, Link, DriftScope } from "../src/viewmodel";
import { SCRY_VERSION } from "../src/viewmodel";
import type { ModelHealthReport, HealthCounts } from "../src/health";

// --- Context tier: people, the platform, and the outside systems ------------

const contextNodes: Node[] = [
  { id: "cardholder", kind: "person", name: "Cardholder", external: true,
    description: "A shopper paying for goods through a merchant's checkout." },
  { id: "merchant", kind: "person", name: "Merchant", external: true,
    description: "A business integrating Aperture Pay to accept card payments." },
  { id: "aperture", kind: "system", name: "Aperture Pay",
    technology: "Payments platform",
    description: "Accepts, authorizes, and settles card payments on behalf of merchants." },
  { id: "acquiring-bank", kind: "system", name: "Acquiring Bank", external: true,
    description: "Settles authorized funds into the merchant's account." },
  { id: "card-networks", kind: "system", name: "Card Networks", external: true,
    description: "Visa / Mastercard authorization rails." },
];

// --- Container tier: the services inside Aperture Pay ------------------------

const containerNodes: Node[] = [
  {
    id: "dashboard", kind: "container", name: "Merchant Dashboard", parentId: "aperture",
    technology: "React", visual: true,
    description: "Where merchants watch payouts, disputes, and live volume.",
  },
  {
    id: "api-gateway", kind: "container", name: "API Gateway", parentId: "aperture",
    technology: "Envoy",
    description: "The single front door — authenticates and routes every request.",
  },
  {
    id: "auth", kind: "container", name: "Auth Service", parentId: "aperture",
    technology: "Go",
    description: "Issues and verifies API keys and merchant session tokens.",
    responsibilities: [
      { id: "r-auth-1", statement: "Verify every request carries a valid, unexpired credential" },
      { id: "r-auth-2", statement: "Scope each token to the merchant account that owns it",
        directives: ["Never widen a token's scope on refresh"] },
    ],
  },
  {
    id: "ledger", kind: "container", name: "Ledger Service", parentId: "aperture",
    technology: "Rust",
    description: "The double-entry source of truth for every movement of money.",
    responsibilities: [
      { id: "r-ledger-1", statement: "Record each authorization and capture as a balanced double-entry" },
      { id: "r-ledger-2", statement: "Hold funds in escrow until the acquiring bank confirms settlement" },
      { id: "r-ledger-3", statement: "Refuse any posting that would leave an account balance negative",
        directives: ["Every write goes through a single serialized transaction"] },
    ],
  },
  {
    id: "fraud", kind: "container", name: "Fraud Engine", parentId: "aperture",
    technology: "Python",
    description: "Scores each payment for risk before the ledger commits it.",
    responsibilities: [
      { id: "r-fraud-1", statement: "Score every payment for risk within the authorization window" },
      { id: "r-fraud-2", statement: "Block a transaction when its risk score exceeds the merchant's threshold" },
    ],
  },
  {
    id: "webhooks", kind: "container", name: "Webhook Dispatcher", parentId: "aperture",
    technology: "Go",
    description: "Delivers payment events to merchant endpoints, with retries.",
    responsibilities: [
      { id: "r-wh-1", statement: "Deliver each payment event to the merchant's endpoint at least once" },
      { id: "r-wh-2", statement: "Retry failed deliveries with exponential backoff for 24 hours" },
    ],
  },
  {
    id: "notifications", kind: "container", name: "Notification Service", parentId: "aperture",
    technology: "Node",
    description: "Emails receipts and dispute alerts to cardholders and merchants.",
  },
  {
    id: "payments-db", kind: "container", name: "Payments DB", parentId: "aperture",
    technology: "PostgreSQL",
    description: "Durable store for accounts, transactions, and the ledger.",
  },
  {
    id: "event-bus", kind: "container", name: "Event Bus", parentId: "aperture",
    technology: "Kafka",
    description: "Carries payment events between services.",
  },
];

// --- Links -------------------------------------------------------------------

const links: Link[] = [
  // Context tier
  { id: "l-ctx-1", src: "cardholder", dst: "aperture", label: "Pays via hosted checkout" },
  { id: "l-ctx-2", src: "merchant", dst: "dashboard", label: "Manages account" },
  { id: "l-ctx-3", src: "aperture", dst: "acquiring-bank", label: "Settles funds" },
  { id: "l-ctx-4", src: "aperture", dst: "card-networks", label: "Authorizes" },

  // Container tier
  { id: "l-1", src: "dashboard", dst: "api-gateway", label: "Calls", method: "HTTPS" },
  { id: "l-2", src: "api-gateway", dst: "auth", label: "Verifies token", method: "gRPC" },
  { id: "l-3", src: "api-gateway", dst: "ledger", label: "Submits payment", method: "gRPC" },
  { id: "l-4", src: "ledger", dst: "fraud", label: "Scores risk", method: "gRPC" },
  { id: "l-5", src: "ledger", dst: "payments-db", label: "Reads / writes", method: "SQL" },
  { id: "l-6", src: "ledger", dst: "event-bus", label: "Publishes events" },
  { id: "l-7", src: "event-bus", dst: "webhooks", label: "Delivers" },
  { id: "l-8", src: "event-bus", dst: "notifications", label: "Delivers" },
  { id: "l-9", src: "fraud", dst: "payments-db", label: "Reads history", method: "SQL" },
  { id: "l-10", src: "auth", dst: "payments-db", label: "Reads accounts", method: "SQL" },
];

/** The fictional payments platform, shaped to the real on-disk model. */
export const paymentsModel: ScryModel = {
  version: SCRY_VERSION,
  nodes: [...contextNodes, ...containerNodes],
  links,
  groups: [],
  // Each responsibility resolves to code — drives the "traced to code" anchors
  // under each claim on the node page.
  sourceMap: {
    "r-ledger-1": [{ pattern: "ledger/src/posting.rs", symbol: "post_entry", line: 48, endLine: 92 }],
    "r-ledger-2": [{ pattern: "ledger/src/escrow.rs", symbol: "hold_in_escrow", line: 21, endLine: 67 }],
    "r-ledger-3": [{ pattern: "ledger/src/posting.rs", symbol: "assert_non_negative", line: 110, endLine: 131 }],
    "r-fraud-1": [{ pattern: "fraud/scoring/score.py", symbol: "score_payment", line: 12, endLine: 58 }],
    "r-fraud-2": [{ pattern: "fraud/scoring/decision.py", symbol: "block_if_over_threshold", line: 9, endLine: 33 }],
    "r-auth-1": [{ pattern: "auth/internal/verify.go", symbol: "VerifyCredential", line: 30, endLine: 71 }],
    "r-wh-1": [{ pattern: "webhooks/dispatch.go", symbol: "Deliver", line: 40, endLine: 88 }],
    "r-wh-2": [{ pattern: "webhooks/retry.go", symbol: "backoff", line: 15, endLine: 52 }],
  },
  boundaries: {
    ledger: [{ pattern: "ledger/**" }],
    fraud: [{ pattern: "fraud/**" }],
    auth: [{ pattern: "auth/**" }],
    webhooks: [{ pattern: "webhooks/**" }],
  },
};

// ===========================================================================
// Scene support — variants of the core model + the derived/observability state
// the lifted components read. All shaped to the real app types.
// ===========================================================================

const clone = (m: ScryModel): ScryModel => structuredClone(m);
const resp = (m: ScryModel, nodeId: string, respId: string) =>
  m.nodes.find((n) => n.id === nodeId)?.responsibilities?.find((r) => r.id === respId);

/** The committed model — the diff base for the node page. It LACKS what the
 *  planned model has, so the missing claims read as "Added" plan marks: the
 *  model leads, the code follows. */
export const committedModel: ScryModel = (() => {
  const m = clone(paymentsModel);
  const ledger = m.nodes.find((n) => n.id === "ledger");
  if (ledger?.responsibilities) {
    // r-ledger-3 (refuse negative balance) is newly planned, not yet committed.
    ledger.responsibilities = ledger.responsibilities.filter((r) => r.id !== "r-ledger-3");
  }
  return m;
})();

/** The model in a drift state — claims awaiting a human verdict. */
export const driftModel: ScryModel = (() => {
  const m = clone(paymentsModel);
  // The semantic check judged the escrow code no longer discharges this claim.
  const stale = resp(m, "ledger", "r-ledger-2");
  if (stale) stale.stale = true;
  // Code does something no claim describes — discovered, awaiting adopt/reject.
  const fraud = m.nodes.find((n) => n.id === "fraud");
  fraud?.responsibilities?.push({
    id: "r-fraud-vagrant",
    statement: "Cache risk scores for repeat cardholders within a 10-minute window",
    vagrant: true,
  });
  return m;
})();

/** Boundary-owning nodes whose code changed since the last reconcile. */
export const driftScopes: DriftScope[] = [
  { nodeId: "ledger", nodeName: "Ledger Service", changedFiles: ["ledger/src/escrow.rs", "ledger/src/posting.rs"] },
];

/** Claims the agent authored this session, awaiting a human's eyes. */
export const newRespIds: ReadonlySet<string> = new Set(["r-wh-2"]);

// --- Health / observability report ------------------------------------------

const counts = (p: Partial<HealthCounts>): HealthCounts => ({
  responsibilities: 0, properties: 0, vagrant: 0, stale: 0,
  anchorable: 0, anchored: 0, unmapped: 0, ...p,
});

/** A derived observability report: coverage per node, anchor drift, and the
 *  implied (code-discovered) connections the diagram draws as ghosts. */
export const healthReport: ModelHealthReport = {
  health: {
    nodes: {
      ledger: {
        own: counts({ responsibilities: 3, anchorable: 3, anchored: 3 }),
        subtree: counts({ responsibilities: 3, anchorable: 3, anchored: 3 }),
        boundary: { totalFiles: 14, anchoredFiles: 11, darkFiles: ["ledger/src/escrow.rs", "ledger/migrations/0007_escrow.sql", "ledger/src/reconcile.rs"] },
      },
      fraud: {
        own: counts({ responsibilities: 2, vagrant: 1, anchorable: 2, anchored: 2 }),
        subtree: counts({ responsibilities: 2, vagrant: 1, anchorable: 2, anchored: 2 }),
        boundary: { totalFiles: 9, anchoredFiles: 8, darkFiles: ["fraud/scoring/cache.py"] },
      },
      auth: {
        own: counts({ responsibilities: 2, anchorable: 2, anchored: 1, unmapped: 1 }),
        subtree: counts({ responsibilities: 2, anchorable: 2, anchored: 1, unmapped: 1 }),
        boundary: { totalFiles: 7, anchoredFiles: 6, darkFiles: ["auth/internal/session.go"] },
      },
    },
    totals: counts({ responsibilities: 7, vagrant: 1, stale: 1, anchorable: 7, anchored: 6, unmapped: 1 }),
  },
  anchors: [
    { key: "r-ledger-2", hostId: "ledger", hostName: "Ledger Service", file: "ledger/src/escrow.rs", symbol: "hold_in_escrow", state: "changed" },
  ],
  reanchored: 2,
  derived: {
    linkAudit: [
      { linkId: "l-3", edgeCount: 14 },
      { linkId: "l-4", edgeCount: 6 },
      { linkId: "l-5", edgeCount: 22 },
    ],
    // Sibling services the code connects but no declared link covers — the
    // diagram draws these as implied-connection ghosts.
    unmodeled: [{ src: "fraud", dst: "event-bus", count: 4 }],
    resolvedEdges: [],
  },
};
