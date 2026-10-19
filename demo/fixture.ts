/**
 * Aperture Pay — the fictional payments platform the film is shot on, as the
 * derived shapes the app reads: a DerivedTree, a DerivedStatus and
 * structure.toml as parsed, with the story (story.ts) laid over them. No real
 * repo and no derivation: the film's fallback until demo/derive.mjs writes
 * demo/fixture.json from a real one.
 *
 * Every claim text is an EARS sentence that passes `scryer-mcp lint`; the
 * script and the story name claims by it.
 */

import { statusOf, treeWith } from "./shapes";
import { STORY_FILES, tell } from "./story";
import type {
  ClaimLevel,
  ClaimState,
  DerivedClaim,
  DerivedViolation,
  Fixture,
  Link,
  Node,
  ProbeMark,
  ScryModel,
  Structure,
} from "./types";

// --- Structure -----------------------------------------------------------------

export const structure: Structure = {
  system: {
    name: "Aperture Pay",
    persons: ["Cardholder", "Merchant"],
    externals: ["Acquiring Bank", "Card Networks"],
  },
  levels: { e2e: ["e2e/**"], integration: ["*/tests/**", "*/integration/**"] },
  containers: {
    ledger: { name: "Ledger", style: "hexagonal" },
    dashboard: { name: "Dashboard", style: "feature-sliced" },
    "api-gateway": { name: "API Gateway" },
    fraud: { name: "Fraud" },
    webhooks: { name: "Webhooks" },
    "payments-db": { name: "Payments DB" },
  },
};

// --- Nodes -----------------------------------------------------------------------

function node(id: string, kind: Node["kind"], name: string, extra: Partial<Node> = {}): Node {
  return { id, kind, name, responsibilities: [], ...extra };
}

const SYSTEM = "sys";

const contextNodes: Node[] = [
  node(SYSTEM, "system", "Aperture Pay", { description: "Takes card payments for merchants and settles them." }),
  node("person-cardholder", "person", "Cardholder", { description: "Pays a merchant by card." }),
  node("person-merchant", "person", "Merchant", { description: "Sells, refunds and reconciles." }),
  node("ext-bank", "system", "Acquiring Bank", { external: true, description: "Authorises and captures card payments." }),
  node("ext-networks", "system", "Card Networks", { external: true, description: "Route authorisations and chargebacks." }),
];

/** Containers with their components: `[id, name, technology, description, components]`. */
const CONTAINERS: [string, string, string, string, [string, string, string?][]][] = [
  ["api-gateway", "API Gateway", "TypeScript · Fastify", "The merchant-facing payments API.", [
    ["routes", "Maps the public API onto the services."],
    ["auth", "Checks merchant keys and scopes."],
    ["idempotency", "Replays a request's first answer."],
  ]],
  ["ledger", "Ledger", "Rust", "The double-entry record of every money movement.", [
    ["domain", "Accounts, entries and the rules that balance them.", "domain"],
    ["application", "Refunds, captures and payouts, and the ports they ask the world for.", "application"],
    ["infrastructure", "Postgres and bank-file implementations of the ports.", "infrastructure"],
  ]],
  ["fraud", "Fraud", "Python", "Scores a payment before it is authorised.", [
    ["rules", "Hard limits merchants and risk set."],
    ["scoring", "The model's risk score for a payment."],
  ]],
  ["webhooks", "Webhooks", "Go", "Tells merchants what happened to their payments.", [
    ["delivery", "Sends events and retries the failures."],
    ["signing", "Signs each event so merchants can trust it."],
  ]],
  ["dashboard", "Dashboard", "TypeScript · React", "Where merchants see and refund payments.", [
    ["pages", "One screen per route.", "pages"],
    ["features", "Refund, export and dispute flows.", "features"],
    ["entities", "Payments, refunds and payouts as the UI holds them.", "entities"],
    ["shared", "UI kit and API client.", "shared"],
  ]],
  ["payments-db", "Payments DB", "PostgreSQL", "Ledger entries and payment state.", []],
];

const containerNodes: Node[] = [];
const componentNodes: Node[] = [];
for (const [id, name, technology, description, comps] of CONTAINERS) {
  containerNodes.push(
    node(id, "container", name, {
      parentId: SYSTEM,
      technology,
      description,
      style: structure.containers[id]?.style,
    }),
  );
  for (const [comp, desc, layer] of comps) {
    componentNodes.push(node(`${id}.${comp}`, "component", comp, { parentId: id, description: desc, layer }));
  }
}

const links: Link[] = [
  ["person-cardholder", SYSTEM, "pays through"],
  ["person-merchant", SYSTEM, "sells and refunds with"],
  [SYSTEM, "ext-bank", "authorises and captures with"],
  ["ext-networks", SYSTEM, "sends chargebacks to"],
  ["api-gateway", "ledger", "posts entries to"],
  ["api-gateway", "fraud", "scores payments with"],
  ["ledger", "payments-db", "stores entries in"],
  ["ledger", "webhooks", "publishes events to"],
  ["dashboard", "api-gateway", "calls"],
  ["ledger.infrastructure", "ledger.application", ""],
  ["ledger.application", "ledger.domain", ""],
  ["dashboard.pages", "dashboard.features", ""],
  ["dashboard.features", "dashboard.entities", ""],
  ["dashboard.entities", "dashboard.shared", ""],
].map(([src, dst, label], i) => ({ id: `link-${i + 1}`, src, dst, label, kind: label ? undefined : "depends" }));

// --- Claims ------------------------------------------------------------------------

/**
 * `[id, node, level, file, line, identifier, junit name, description, concern?, state, probe?]`.
 * Unit claims sit on a component, integration claims on a container, e2e
 * claims on the system.
 */
type Row = [string, string, ClaimLevel, string, number, string, string, string, string | null, ClaimState, ProbeMark?];

const ROWS: Row[] = [
  // System (e2e)
  ["c-e2e-1", SYSTEM, "system", "e2e/checkout.spec.ts", 12, "pays by card", "checkout > pays by card", "When a cardholder pays by card, captures the payment and credits the merchant", null, "proven"],
  ["c-e2e-2", SYSTEM, "system", "e2e/checkout.spec.ts", 41, "declines a stolen card", "checkout > declines a stolen card", "If the bank declines the card, then shows the cardholder a retry", null, "proven"],
  ["c-e2e-3", SYSTEM, "system", "e2e/refunds.spec.ts", 9, "refunds in full", "refunds > refunds in full", "When a merchant refunds a payment, returns the money to the card", null, "proven"],
  ["c-e2e-4", SYSTEM, "system", "e2e/payouts.spec.ts", 15, "pays out daily", "payouts > pays out daily", "Pays each merchant's settled balance out once a day", null, "proven"],
  ["c-e2e-5", SYSTEM, "system", "e2e/disputes.spec.ts", 22, "opens a dispute", "disputes > opens a dispute", "When a chargeback arrives, opens a dispute the merchant can answer", null, "proven"],

  // API Gateway
  ["c-gw-1", "api-gateway", "container", "api-gateway/tests/payments.int.ts", 18, "creates a payment", "payments api > creates a payment", "When a merchant creates a payment, answers with its id and status", null, "proven"],
  ["c-gw-2", "api-gateway", "container", "api-gateway/tests/payments.int.ts", 47, "stores the request", "payments api > stores the request", "Records every accepted request before answering it", "persistence", "proven"],
  ["c-gw-3", "api-gateway.routes", "component", "api-gateway/src/routes/payments.test.ts", 8, "routes captures", "routes > routes captures", "Routes a capture to the ledger", null, "proven"],
  ["c-gw-4", "api-gateway.routes", "component", "api-gateway/src/routes/payments.test.ts", 25, "rejects unknown fields", "routes > rejects unknown fields", "If a request carries an unknown field, then rejects it with a 400", null, "proven"],
  ["c-gw-5", "api-gateway.auth", "component", "api-gateway/src/auth/keys.test.ts", 11, "rejects a revoked key", "keys > rejects a revoked key", "If the merchant key is revoked, then answers 401", null, "proven"],
  ["c-gw-6", "api-gateway.auth", "component", "api-gateway/src/auth/scopes.test.ts", 7, "limits refund scope", "scopes > limits refund scope", "While a key lacks the refund scope, refuses refunds", null, "proven"],
  ["c-gw-7", "api-gateway.idempotency", "component", "api-gateway/src/idempotency/replay.test.ts", 14, "replays the first answer", "replay > replays the first answer", "When a request repeats its idempotency key, replays the first answer", null, "proven"],
  ["c-gw-9", "api-gateway.routes", "component", "api-gateway/src/routes/refunds.test.ts", 10, "routes refunds", "routes > routes refunds", "Routes a refund to the ledger", null, "proven"],
  ["c-gw-8", "api-gateway.idempotency", "component", "api-gateway/src/idempotency/store.test.ts", 9, "keeps keys a day", "store > keeps keys a day", "Keeps each idempotency key for twenty-four hours", "persistence", "proven"],

  // Ledger
  ["c-led-1", "ledger", "container", "ledger/tests/posting.rs", 12, "posts_balanced_entries", "posts_balanced_entries", "Posts every money movement as balanced entries", null, "proven"],
  ["c-led-2", "ledger", "container", "ledger/tests/posting.rs", 38, "survives_a_restart", "survives_a_restart", "When the service restarts, keeps every posted entry", "persistence", "proven"],
  ["c-led-3", "ledger.domain", "component", "ledger/src/domain/entry.rs", 64, "rejects_unbalanced", "domain::entry::tests::rejects_unbalanced", "If debits and credits differ, then rejects the posting", null, "proven"],
  ["c-led-4", "ledger.domain", "component", "ledger/src/domain/account.rs", 51, "derives_balance", "domain::account::tests::derives_balance", "Derives an account's balance from its entries", null, "proven"],
  ["c-led-5", "ledger.domain", "component", "ledger/src/domain/capture.rs", 40, "captures_up_to_authorised", "domain::capture::tests::captures_up_to_authorised", "If a capture exceeds the authorised amount, then rejects it", null, "proven"],
  ["c-led-6", "ledger.domain", "component", "ledger/src/domain/payout.rs", 33, "nets_fees", "domain::payout::tests::nets_fees", "Nets fees out of each payout", null, "stale"],
  ["c-led-7", "ledger.application", "component", "ledger/src/application/store.rs", 22, "appends_only", "application::store::tests::appends_only", "Exposes entries as an append-only store", null, "proven"],
  ["c-led-8", "ledger.infrastructure", "component", "ledger/src/infrastructure/postgres.rs", 88, "writes_in_one_transaction", "infrastructure::postgres::tests::writes_in_one_transaction", "Writes a posting's entries in one transaction", "persistence", "proven"],
  ["c-led-9", "ledger.infrastructure", "component", "ledger/src/infrastructure/bank_file.rs", 45, "parses_settlement_files", "infrastructure::bank_file::tests::parses_settlement_files", "When a settlement file arrives, matches each line to a capture", null, "proven"],
  ["c-led-10", "ledger.domain", "component", "ledger/src/domain/payout.rs", 71, "holds_disputed_funds", "domain::payout::tests::holds_disputed_funds", "While a dispute is open, holds the disputed amount back from payouts", null, "planned"],

  // Fraud
  ["c-fr-1", "fraud", "container", "fraud/tests/test_scoring_api.py", 10, "test_scores_within_budget", "test_scoring_api::test_scores_within_budget", "Scores a payment within fifty milliseconds", null, "proven"],
  ["c-fr-2", "fraud", "container", "fraud/tests/test_scoring_api.py", 34, "test_keeps_decisions", "test_scoring_api::test_keeps_decisions", "Records every decision with the score behind it", "persistence", "proven"],
  ["c-fr-3", "fraud.rules", "component", "fraud/rules/test_limits.py", 8, "test_blocks_over_limit", "test_limits::test_blocks_over_limit", "If a payment exceeds the merchant's limit, then blocks it", null, "proven"],
  ["c-fr-4", "fraud.rules", "component", "fraud/rules/test_velocity.py", 12, "test_flags_velocity", "test_velocity::test_flags_velocity", "When a card pays five times in a minute, flags the sixth", null, "proven"],
  ["c-fr-5", "fraud.rules", "component", "fraud/rules/test_lists.py", 6, "test_blocks_listed_cards", "test_lists::test_blocks_listed_cards", "Blocks cards on the merchant's block list", null, "proven"],
  ["c-fr-9", "fraud.rules", "component", "fraud/rules/test_lists.py", 28, "test_allows_listed_cards", "test_lists::test_allows_listed_cards", "Lets cards on the merchant's allow list skip the velocity rule", null, "proven"],
  ["c-fr-6", "fraud.scoring", "component", "fraud/scoring/test_model.py", 15, "test_scores_unit_range", "test_model::test_scores_unit_range", "Scores every payment between zero and one", null, "proven"],
  ["c-fr-7", "fraud.scoring", "component", "fraud/scoring/test_model.py", 39, "test_falls_back", "test_model::test_falls_back", "If the model is unavailable, then scores with the rules alone", null, "proven"],
  ["c-fr-8", "fraud.scoring", "component", "fraud/scoring/test_features.py", 21, "test_caches_features", "test_features::test_caches_features", "Caches a card's features for an hour", "persistence", "proven"],

  // Webhooks
  ["c-wh-1", "webhooks", "container", "webhooks/integration/delivery_test.go", 14, "TestDeliversInOrder", "TestDeliversInOrder", "Delivers a merchant's events in the order they happened", null, "proven"],
  ["c-wh-2", "webhooks", "container", "webhooks/integration/delivery_test.go", 52, "TestOutboxSurvivesCrash", "TestOutboxSurvivesCrash", "When the sender crashes, delivers the outbox on restart", "persistence", "proven"],
  ["c-wh-3", "webhooks.delivery", "component", "webhooks/delivery/retry_test.go", 9, "TestRetriesWithBackoff", "TestRetriesWithBackoff", "If a merchant endpoint fails, then retries with backoff for a day", null, "proven"],
  ["c-wh-4", "webhooks.delivery", "component", "webhooks/delivery/retry_test.go", 40, "TestStopsOnGone", "TestStopsOnGone", "When an endpoint answers 410, stops sending to it", null, "proven"],
  ["c-wh-5", "webhooks.delivery", "component", "webhooks/delivery/queue_test.go", 18, "TestPersistsQueue", "TestPersistsQueue", "Queues each event durably before sending it", "persistence", "proven"],
  ["c-wh-6", "webhooks.signing", "component", "webhooks/signing/sign_test.go", 11, "TestSignsBody", "TestSignsBody", "Signs each event body with the merchant's secret", null, "proven"],
  ["c-wh-7", "webhooks.signing", "component", "webhooks/signing/sign_test.go", 33, "TestRotatesSecrets", "TestRotatesSecrets", "While a secret rotates, signs with both the old and the new one", null, "proven"],
  ["c-wh-9", "webhooks.signing", "component", "webhooks/signing/sign_test.go", 58, "TestStampsSigningTime", "TestStampsSigningTime", "Stamps each event with the time it was signed", null, "proven"],
  ["c-wh-8", "webhooks.delivery", "component", "webhooks/delivery/retry_test.go", 71, "TestRetriesLedgerEvents", "TestRetriesLedgerEvents", "When a ledger event fails to send, retries it before newer events", null, "planned"],

  // Dashboard
  ["c-db-1", "dashboard", "container", "dashboard/tests/refund-flow.int.test.tsx", 20, "refunds from the payment page", "refund flow > refunds from the payment page", "When a merchant confirms a refund, shows it pending on the payment", null, "proven"],
  ["c-db-2", "dashboard", "container", "dashboard/tests/session.int.test.tsx", 12, "keeps filters", "session > keeps filters", "Keeps a merchant's filters across visits", "persistence", "proven"],
  ["c-db-3", "dashboard.pages", "component", "dashboard/src/pages/payments.test.tsx", 9, "lists recent payments", "payments page > lists recent payments", "Lists the last fifty payments newest first", null, "proven"],
  ["c-db-4", "dashboard.pages", "component", "dashboard/src/pages/payout.test.tsx", 14, "shows the next payout", "payout page > shows the next payout", "Shows the next payout's amount and date", null, "proven"],
  ["c-db-5", "dashboard.features", "component", "dashboard/src/features/refund/refund.test.tsx", 17, "caps the refund", "refund > caps the refund", "If the refund exceeds what is left, then disables confirm", null, "proven"],
  ["c-db-6", "dashboard.features", "component", "dashboard/src/features/export/export.test.ts", 8, "exports csv", "export > exports csv", "Exports the filtered payments as CSV", null, "proven"],
  ["c-db-7", "dashboard.features", "component", "dashboard/src/features/dispute/dispute.test.tsx", 21, "uploads evidence", "dispute > uploads evidence", "When a merchant answers a dispute, uploads the evidence with it", null, "proven"],
  ["c-db-8", "dashboard.entities", "component", "dashboard/src/entities/payment.test.ts", 6, "formats amounts", "payment > formats amounts", "Formats amounts in the payment's currency", null, "proven"],
  ["c-db-9", "dashboard.entities", "component", "dashboard/src/entities/refund.test.ts", 10, "derives the remaining amount", "refund > derives the remaining amount", "Derives what is left to refund from past refunds", null, "proven"],
  ["c-db-10", "dashboard.shared", "component", "dashboard/src/shared/api.test.ts", 12, "retries once", "api client > retries once", "If a request times out, then retries it once", null, "proven"],
  ["c-db-11", "dashboard.shared", "component", "dashboard/src/shared/cache.test.ts", 7, "caches responses", "cache > caches responses", "Caches each response in session storage", "persistence", "proven"],
  ["c-db-13", "dashboard.pages", "component", "dashboard/src/pages/disputes.test.tsx", 11, "lists open disputes", "disputes page > lists open disputes", "Lists open disputes with their deadlines", null, "proven"],
  ["c-db-12", "dashboard.features", "component", "dashboard/src/features/refund/refund.test.tsx", 52, "explains a declined refund", "refund > explains a declined refund", "If the bank declines a refund, then shows the merchant why", null, "proven", "crosshair"],
];

/**
 * Act 1's plan: the five todo tests the agent writes for partial refunds, in
 * `ledger/tests/refunds.rs`. The user deletes the fifth; the other four are
 * built in act 3. `c-ref-2` is the one act 4 probes.
 */
export const REFUND_STUBS: DerivedClaim[] = [
  ["c-ref-1", 8, "refunds_part_of_a_capture", "When a merchant refunds part of a capture, posts the refund against it"],
  ["c-ref-2", 22, "rejects_over_refund", "When a refund exceeds the captured amount, rejects it"],
  ["c-ref-3", 36, "tracks_refundable_remainder", "Tracks the refundable remainder of each capture"],
  ["c-ref-4", 49, "refunds_in_capture_currency", "Refunds in the currency of the capture"],
  ["c-ref-5", 61, "notifies_on_refund", "When a refund posts, notifies the merchant"],
].map(([id, line, identifier, description]) => ({
  id: id as string,
  nodeId: "Ledger",
  level: "container",
  file: "ledger/tests/refunds.rs",
  line: line as number,
  identifier: identifier as string,
  description: description as string,
  junitName: identifier as string,
  planned: true,
}));

/** Act 2's misplaced import: a domain module reaching into infrastructure. */
export const STYLE_VIOLATION: DerivedViolation = {
  nodeId: "Ledger/domain",
  other: "Ledger/application",
  file: "ledger/src/domain/memory.rs",
  kind: "forbidden_link",
  detail: "declared link: 'domain' (domain) → 'application' (application) is illegal in style 'hexagonal': domain may depend on domain",
};

// --- Diagram placements ------------------------------------------------------------

// --- Assembly ------------------------------------------------------------------------

const claims: DerivedClaim[] = ROWS.map(([id, nodeId, level, file, line, identifier, junitName, description, concern, state]) => ({
  id,
  nodeId,
  level,
  file,
  line,
  identifier,
  description,
  concerns: concern ? [concern] : undefined,
  junitName,
  planned: state === "planned" || undefined,
}));

/** The model with no claims on it yet: nodes, links, the concern registry. */
const skeleton: ScryModel = {
  version: "0.3",
  groups: [],
  nodes: [...contextNodes, ...containerNodes, ...componentNodes],
  links,
  concerns: [{ slug: "persistence", description: "What survives a restart, and how." }],
};

/** Every source file the film names, for the tree's file map. */
const SOURCE_FILES = [
  ...ROWS.map((r) => r[3]),
  ...STORY_FILES,
  "ledger/src/domain/memory.rs",
  "ledger/src/infrastructure/memory.rs",
  "fraud/scoring/legacy.py",
  "ledger/src/application/refunds.rs",
  "ledger/src/application/store.rs",
  "ledger/src/domain/account.rs",
  "ledger/src/domain/capture.rs",
  "ledger/src/domain/entry.rs",
  "ledger/src/domain/payout.rs",
  "ledger/src/domain/refund.rs",
  "ledger/src/infrastructure/bank_file.rs",
  "ledger/src/infrastructure/postgres.rs",
  "ledger/src/post.rs",
  "webhooks/delivery/debug.go",
  "webhooks/delivery/retry.go",
];

/** The deepest node a file's path names (`ledger/src/domain/x.rs` →
 *  `ledger.domain`), else its container — what the derivation's file map says. */
function nodeOfFile(file: string): string | undefined {
  const parts = file.split("/");
  const ids = new Set(skeleton.nodes.map((n) => n.id));
  for (const seg of parts.slice(1).reverse()) {
    if (ids.has(`${parts[0]}.${seg}`)) return `${parts[0]}.${seg}`;
  }
  return ids.has(parts[0]) ? parts[0] : undefined;
}

// --- The fixture -------------------------------------------------------------------

const tree = treeWith(
  {
    fingerprint: 0xa9e7,
    committed: skeleton,
    planned: skeleton,
    claims: [],
    fileNodes: Object.fromEntries(SOURCE_FILES.flatMap((file) => [[file, nodeOfFile(file)]].filter(([, n]) => n))),
    darkFiles: ["fraud/scoring/legacy.py", "webhooks/delivery/debug.go"],
    styleViolations: [],
  },
  claims,
  [],
);

/** The poster frame's world: mostly proven, a few planned, one stale, one
 *  crosshair, and the sessions that left it so. */
export const fixture: Fixture = tell({
  tree,
  status: statusOf(
    tree,
    Object.fromEntries(ROWS.map((r) => [r[0], r[9]])),
    Object.fromEntries(ROWS.filter((r) => r[10]).map((r) => [r[0], r[10] as ProbeMark])),
    {},
  ),
  structure,
  sessions: [],
  layout: {},
}).fixture;

export default fixture;
