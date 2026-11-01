/**
 * The concern lens: its subtree tally (`concernCounts`) and its options (`modelConcerns`).
 */
import { describe, expect, it } from "vitest";
import {
  concernCounts,
  concernGapCounts,
  emptyModel,
  modelConcerns,
  type Node,
  type Claim,
  type ScryModel,
} from "../src/entities/model/viewmodel";

let respSeq = 0;
const resp = (...concerns: string[]): Claim => ({
  id: `resp-${++respSeq}`,
  statement: "does a thing",
  ...(concerns.length > 0 ? { concerns } : {}),
});

const node = (id: string, parentId: string | undefined, resps: Claim[]): Node => ({
  id,
  kind: "component",
  name: id,
  ...(parentId ? { parentId } : {}),
  claims: resps,
});

const modelWith = (nodes: Node[]): ScryModel => ({ ...emptyModel(), nodes });

describe("concernCounts", () => {
  it("rolls own counts up the ancestor chain; absent id means unlit", () => {
    const m = modelWith([
      node("sys", undefined, []),
      node("api", "sys", [resp("auth")]),
      node("webhook", "api", [resp("auth"), resp("idempotency")]),
      node("ui", "sys", []),
    ]);
    const counts = concernCounts(m, "auth");
    expect(counts.get("webhook")).toBe(1);
    expect(counts.get("api")).toBe(2);
    expect(counts.get("sys")).toBe(2);
    expect(counts.has("ui")).toBe(false);
  });

  it("Lights the symbols a tagged claim's test exercises and their components, counting each claim once", () => {
    const login = resp("auth");
    const m = modelWith([
      node("sys", undefined, []),
      node("api", "sys", [login]),
      node("session", "sys", []),
      { ...node("verify", "session", []), kind: "symbol", exercisedBy: [login.id] },
      { ...node("own", "api", []), kind: "symbol", exercisedBy: [login.id] },
    ]);
    const counts = concernCounts(m, "auth");
    expect(counts.get("verify")).toBe(1);
    expect(counts.get("session")).toBe(1);
    expect(counts.get("api")).toBe(1);
    expect(counts.get("sys")).toBe(1);
  });

  it("Counts a claim under each concern it is tagged with", () => {
    const m = modelWith([node("webhook", undefined, [resp("auth", "idempotency"), resp("auth")])]);
    expect(concernCounts(m, "auth").get("webhook")).toBe(2);
    expect(concernCounts(m, "idempotency").get("webhook")).toBe(1);
    expect(modelConcerns(m)).toEqual([
      { slug: "auth", count: 2 },
      { slug: "idempotency", count: 1 },
    ]);
  });
});

describe("modelConcerns", () => {
  it("Lists the concerns the model's claims carry, A to Z, each with its claim count", () => {
    const m = modelWith([
      node("api", undefined, [resp("validation"), resp("auth"), resp()]),
      node("webhook", "api", [resp("auth")]),
    ]);
    expect(modelConcerns(m)).toEqual([
      { slug: "auth", count: 2 },
      { slug: "validation", count: 1 },
    ]);
  });
});

describe("concernGapCounts", () => {
  it("Rolls each node's gap lines for a concern up its ancestors; a node with none is absent", () => {
    const m = modelWith([
      node("sys", undefined, []),
      node("session", "sys", []),
      node("store", "sys", []),
      node("ui", "sys", []),
    ]);
    const status = {
      session: { concernGaps: { auth: 3 } },
      store: { concernGaps: { auth: 1, persistence: 4 } },
      ui: {},
    };
    const gaps = concernGapCounts(m, status, "auth");
    expect(gaps.get("session")).toBe(3);
    expect(gaps.get("store")).toBe(1);
    expect(gaps.get("sys")).toBe(4);
    expect(gaps.has("ui")).toBe(false);
    expect(concernGapCounts(m, undefined, "auth").size).toBe(0);
  });
});
