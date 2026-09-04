/**
 * The inbox stream (`src/inbox.ts`) — the pure merge of every verdict source
 * into one ordered list of cards.
 *
 * What these pin: tier-then-recency ordering; a card leaves the moment its
 * source does (a resolved vagrant, a fold that later succeeds); the change
 * pin filters by the ledger tag; a concern-tagged claim outranks its source
 * tier; and an amendment card carries the approved text beside the amended.
 */
import { describe, expect, it } from "vitest";
import type { ClaimProbeStatus, ClaimTestStatus } from "../src/health";
import {
  buildInboxCards,
  closeGateItems,
  filterByChange,
  inboxKeyAction,
  inboxUnread,
  type InboxCard,
  type InboxInput,
  type Refusal,
} from "../src/inbox";
import { planDiff } from "../src/planDiff";
import type { ScryModel } from "../src/viewmodel";

type N = ScryModel["nodes"][number];
type R = NonNullable<N["responsibilities"]>[number];

const model = (nodes: N[], extra: Partial<ScryModel> = {}): ScryModel => ({
  version: "0.3",
  nodes,
  links: [],
  groups: [],
  ...extra,
});

const node = (id: string, extra: Partial<N> = {}): N =>
  ({ id, kind: "component", name: id.toUpperCase(), ...extra }) as N;

const resp = (id: string, statement: string, extra: Partial<R> = {}): R => ({ id, statement, ...extra });

const input = (partial: Partial<InboxInput> & { model: ScryModel }): InboxInput => ({
  committed: partial.committed ?? partial.model,
  planDiff: partial.planDiff ?? planDiff(partial.committed ?? partial.model, partial.model),
  verdicts: {},
  probes: {},
  refusals: [],
  closeGate: [],
  now: 10_000,
  ...partial,
});

const verdict = (respId: string, outcome: ClaimTestStatus["outcome"], recordedAt: number): ClaimTestStatus => ({
  respId,
  outcome,
  cases: 1,
  stale: false,
  recordedAt,
});

const probe = (respId: string, survived: number, recordedAt: number): ClaimProbeStatus => ({
  respId,
  probes: 3,
  survived,
  survivors: survived > 0 ? ["flipped the guard"] : [],
  stale: false,
  recordedAt,
});

describe("buildInboxCards", () => {
  it("orders by tier, then newest first within a tier", () => {
    const m = model([
      node("a", {
        responsibilities: [
          // two vagrants, different ages — newest first within the tier
          resp("v-old", "does old thing", { vagrant: true, lastTouchedAt: 100 }),
          resp("v-new", "does new thing", { vagrant: true, lastTouchedAt: 200 }),
          // an amendment (higher tier) that is OLDER than both vagrants
          resp("am", "amended text", {
            vagrant: true,
            vagrantOrigin: "amendment",
            approvedStatement: "approved text",
            lastTouchedAt: 50,
          }),
          // stale (lower tier) but newest of all
          resp("st", "stale claim", { stale: true, lastTouchedAt: 900 }),
          // a survivor (above amendments) and a failing verdict (below stale)
          resp("sv", "probed claim"),
          resp("fl", "failing claim"),
        ],
      }),
    ]);
    const cards = buildInboxCards(
      input({
        model: m,
        probes: { sv: probe("sv", 1, 300) },
        verdicts: { fl: verdict("fl", "failed", 950) },
      }),
    );
    expect(cards.map((c) => `${c.tier}:${c.respId}`)).toEqual([
      "survivor:sv",
      "amendment:am",
      "vagrant:v-new",
      "vagrant:v-old",
      "stale:st",
      "failing:fl",
    ]);
  });

  it("drops a card once its source is resolved", () => {
    const before = model([
      node("a", { responsibilities: [resp("v", "found in code", { vagrant: true })] }),
    ]);
    expect(buildInboxCards(input({ model: before })).map((c) => c.id)).toEqual(["vagrant-claim:v"]);

    // Adopted: the flag clears and the claim is ordinary intent.
    const after = model([node("a", { responsibilities: [resp("v", "found in code")] })]);
    expect(buildInboxCards(input({ model: after }))).toEqual([]);
  });

  it("clears a refused fold's card when the refusals ledger no longer holds it", () => {
    const m = model([node("a", { responsibilities: [resp("r1", "when asked, answers")] })]);
    const refusal: Refusal = {
      respId: "r1",
      hostId: "a",
      kind: "no-test",
      reason: "no test attached",
      at: 500,
    };
    const withRefusal = buildInboxCards(input({ model: m, refusals: [refusal] }));
    expect(withRefusal).toHaveLength(1);
    expect(withRefusal[0]).toMatchObject({ tier: "refused", kind: "refused", respId: "r1", at: 500 });
    expect(withRefusal[0].actions.map((a) => a.kind)).toEqual(["open"]);

    // The agent attached a test and folded: the backend removed the refusal.
    expect(buildInboxCards(input({ model: m, refusals: [] }))).toEqual([]);
  });

  it("filters the stream to a pinned change via the ledger tag", () => {
    const m = model(
      [
        node("a", {
          responsibilities: [
            resp("x", "in chg-1", { vagrant: true, vagrantOrigin: "addition" }),
            resp("y", "in chg-2", { vagrant: true, vagrantOrigin: "addition" }),
            resp("z", "unfiled", { vagrant: true }),
          ],
        }),
      ],
      {
        changes: [
          { id: "chg-1", rationale: "one", createdAt: 1 },
          { id: "chg-2", rationale: "two", createdAt: 2 },
        ],
        changeMap: { "resp:x": "chg-1", "resp:y": "chg-2" },
      },
    );
    const cards = buildInboxCards(input({ model: m }));
    expect(cards).toHaveLength(3);
    expect(filterByChange(cards, "chg-1").map((c) => c.respId)).toEqual(["x"]);
    expect(filterByChange(cards, "chg-2").map((c) => c.respId)).toEqual(["y"]);
    expect(filterByChange(cards, null)).toHaveLength(3);
  });

  it("promotes a card on a concern-tagged claim to the concern tier", () => {
    const m = model([
      node("a", {
        responsibilities: [
          resp("plain", "plain vagrant", { vagrant: true, lastTouchedAt: 10 }),
          resp("auth", "auth vagrant", { vagrant: true, concern: "auth", lastTouchedAt: 5 }),
        ],
      }),
    ]);
    const cards = buildInboxCards(input({ model: m }));
    expect(cards.map((c) => [c.respId, c.tier, c.kind])).toEqual([
      ["auth", "concern", "vagrant-claim"],
      ["plain", "vagrant", "vagrant-claim"],
    ]);
  });

  it("does not demote a card already above the concern tier", () => {
    const m = model([
      node("sys", {
        kind: "system",
        responsibilities: [resp("c", "new wording", { concern: "auth", lastTouchedAt: 1 })],
      }),
    ]);
    const committed = model([node("sys", { kind: "system", responsibilities: [resp("c", "old wording")] })]);
    const cards = buildInboxCards(input({ model: m, committed }));
    expect(cards).toHaveLength(1);
    expect(cards[0]).toMatchObject({ tier: "contract", kind: "contract-reword", before: "old wording", after: "new wording" });
  });

  it("gives amendment cards the approved and amended text, additions only the new text", () => {
    const m = model([
      node("a", {
        responsibilities: [
          resp("am", "**When** asked, **answers** twice", {
            vagrant: true,
            vagrantOrigin: "amendment",
            approvedStatement: "**When** asked, **answers** once",
          }),
          resp("ad", "**When** idle, **sleeps**", { vagrant: true, vagrantOrigin: "addition" }),
        ],
      }),
    ]);
    const cards = buildInboxCards(input({ model: m }));
    const am = cards.find((c) => c.respId === "am")!;
    const ad = cards.find((c) => c.respId === "ad")!;
    expect(am).toMatchObject({
      tier: "amendment",
      kind: "amendment",
      before: "**When** asked, **answers** once",
      after: "**When** asked, **answers** twice",
    });
    expect(am.actions.map((a) => a.kind)).toEqual(["adopt", "reject", "reword"]);
    expect(ad).toMatchObject({ tier: "amendment", kind: "addition", after: "**When** idle, **sleeps**" });
    expect(ad.before).toBeUndefined();
    // Neither is a code-discovered vagrant card.
    expect(cards.some((c) => c.kind === "vagrant-claim")).toBe(false);
  });

  it("leaves untested claims out — they are a standing list, not a verdict", () => {
    const m = model([node("a", { responsibilities: [resp("u", "**When** poked, **replies**")] })]);
    expect(buildInboxCards(input({ model: m }))).toEqual([]);
  });

  it("flattens a close-gate event and resolves the claim's host", () => {
    const m = model([
      node("comp", { responsibilities: [resp("r1", "when called, records")] }),
    ]);
    const items = closeGateItems(
      {
        session: "s1",
        needsReconcile: [{ file: "src/a.ts", claims: [{ id: "r1", host: "COMP", symbol: "record", state: "changed" }] }],
      },
      777,
    );
    const cards = buildInboxCards(input({ model: m, closeGate: items }));
    expect(cards).toHaveLength(1);
    expect(cards[0]).toMatchObject({
      tier: "close-gate",
      nodeId: "comp",
      respId: "r1",
      at: 777,
      id: "close-gate:s1:src/a.ts:r1",
    });
    expect(cards[0].actions.map((a) => a.kind)).toEqual(["holds", "reword", "flag"]);
  });
});

describe("inboxUnread", () => {
  it("counts the cards not yet seen", () => {
    const m = model([
      node("a", {
        responsibilities: [
          resp("v1", "one", { vagrant: true }),
          resp("v2", "two", { vagrant: true }),
        ],
      }),
    ]);
    const cards = buildInboxCards(input({ model: m }));
    expect(inboxUnread(cards, new Set())).toBe(2);
    expect(inboxUnread(cards, new Set(["vagrant-claim:v1"]))).toBe(1);
    expect(inboxUnread(cards, new Set(cards.map((c) => c.id)))).toBe(0);
  });
});

describe("inboxKeyAction", () => {
  const card = (id: string, actions: InboxCard["actions"]): InboxCard =>
    ({ id, tier: "vagrant", kind: "vagrant", nodeId: "n", nodeName: "N", at: 1, title: id, evidence: {}, actions }) as unknown as InboxCard;
  const adopt = { kind: "adopt", label: "Adopt" } as InboxCard["actions"][number];
  const cards = [card("c1", [adopt]), card("c2", [])];

  it("moves the focus with j/k and clamps at both ends", () => {
    expect(inboxKeyAction("j", cards, 0)).toEqual({ type: "focus", index: 1 });
    expect(inboxKeyAction("j", cards, 1)).toEqual({ type: "focus", index: 1 });
    expect(inboxKeyAction("k", cards, 1)).toEqual({ type: "focus", index: 0 });
    expect(inboxKeyAction("k", cards, 0)).toEqual({ type: "focus", index: 0 });
  });

  it("opens the focused card on Enter and runs a/r/e only when the card carries that action", () => {
    expect(inboxKeyAction("Enter", cards, 0)).toEqual({ type: "open", card: cards[0] });
    expect(inboxKeyAction("a", cards, 0)).toEqual({ type: "run", card: cards[0], action: adopt });
    expect(inboxKeyAction("r", cards, 0)).toBeNull();
    expect(inboxKeyAction("e", cards, 0)).toBeNull();
    expect(inboxKeyAction("a", cards, 1)).toBeNull();
    expect(inboxKeyAction("Enter", cards, 5)).toBeNull();
    expect(inboxKeyAction("x", cards, 0)).toBeNull();
  });
});
