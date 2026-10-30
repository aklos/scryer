/**
 * The concern lens's subtree tally (`concernCounts`).
 */
import { describe, expect, it } from "vitest";
import {
  concernCounts,
  emptyModel,
  type Node,
  type Responsibility,
  type ScryModel,
} from "../src/entities/model/viewmodel";

let respSeq = 0;
const resp = (concern?: string): Responsibility => ({
  id: `resp-${++respSeq}`,
  statement: "does a thing",
  ...(concern !== undefined ? { concern } : {}),
});

const node = (id: string, parentId: string | undefined, resps: Responsibility[]): Node => ({
  id,
  kind: "component",
  name: id,
  ...(parentId ? { parentId } : {}),
  responsibilities: resps,
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
});
