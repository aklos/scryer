/**
 * The concern lens: its subtree tally (`concernCounts`) and its options (`modelConcerns`).
 */
import { describe, expect, it } from "vitest";
import {
  concernCounts,
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
