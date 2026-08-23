/**
 * The claim row's test lane: tone and words for a recorded verdict
 * (`src/health.ts`). Quiet is the norm — green and unrecorded read the same
 * at rest — and stale outranks failing, because a red verdict the code moved
 * past is outdated, not an alarm.
 */
import { describe, expect, it } from "vitest";
import {
  subtreeTestTone,
  testLaneTitle,
  testLaneTone,
  type ClaimTestStatus,
} from "../src/health";
import type { Node, Group, Responsibility } from "../src/viewmodel";

const verdict = (over: Partial<ClaimTestStatus>): ClaimTestStatus => ({
  respId: "r1",
  outcome: "passed",
  cases: 1,
  stale: false,
  recordedAt: 0,
  ...over,
});

describe("testLaneTone", () => {
  it("is quiet for green and for no recorded verdict alike", () => {
    expect(testLaneTone(undefined)).toBe("quiet");
    expect(testLaneTone(verdict({ outcome: "passed" }))).toBe("quiet");
    expect(testLaneTone(verdict({ outcome: "skipped" }))).toBe("quiet");
  });

  it("is failing only while the red verdict is current", () => {
    expect(testLaneTone(verdict({ outcome: "failed" }))).toBe("failing");
    expect(testLaneTone(verdict({ outcome: "errored" }))).toBe("failing");
  });

  it("stale outranks failing — an outdated red verdict is not an alarm", () => {
    expect(testLaneTone(verdict({ outcome: "failed", stale: true }))).toBe("stale");
    expect(testLaneTone(verdict({ outcome: "passed", stale: true }))).toBe("stale");
  });
});

describe("subtreeTestTone", () => {
  const resp = (id: string): Responsibility => ({ id, statement: id });
  const node = (id: string, parentId: string | undefined, resps: string[]): Node => ({
    id,
    kind: "component",
    name: id,
    parentId,
    responsibilities: resps.map(resp),
  });
  // root ── mid ── leaf, plus a group under mid and an unrelated sibling.
  const model = {
    nodes: [
      node("root", undefined, []),
      node("mid", "root", ["r-mid"]),
      node("leaf", "mid", ["r-leaf"]),
      node("outside", undefined, ["r-out"]),
    ],
    groups: [
      { id: "g1", name: "g", memberIds: [], parentNodeId: "mid", responsibilities: [resp("r-group")] } as Group,
    ],
  };

  it("rolls a failing verdict up from any depth, groups included", () => {
    expect(
      subtreeTestTone(model, "root", { "r-leaf": verdict({ outcome: "failed" }) }),
    ).toBe("failing");
    expect(
      subtreeTestTone(model, "root", { "r-group": verdict({ outcome: "errored" }) }),
    ).toBe("failing");
  });

  it("failing outranks stale at the rollup; stale beats quiet", () => {
    expect(
      subtreeTestTone(model, "root", {
        "r-mid": verdict({ stale: true }),
        "r-leaf": verdict({ outcome: "failed" }),
      }),
    ).toBe("failing");
    expect(subtreeTestTone(model, "root", { "r-mid": verdict({ stale: true }) })).toBe("stale");
  });

  it("stays quiet on green subtrees and ignores verdicts outside the scope", () => {
    expect(subtreeTestTone(model, "root", { "r-leaf": verdict({}) })).toBe("quiet");
    expect(subtreeTestTone(model, "mid", { "r-out": verdict({ outcome: "failed" }) })).toBe(
      "quiet",
    );
  });
});

describe("testLaneTitle", () => {
  it("leads with the attachment count, then the verdict in words", () => {
    expect(testLaneTitle(1, undefined)).toBe("1 test attached — no run recorded yet.");
    expect(testLaneTitle(3, verdict({ outcome: "passed", cases: 4 }))).toBe(
      "3 tests attached — passing (4 cases).",
    );
  });

  it("says stale in words when the code moved past the verdict", () => {
    expect(testLaneTitle(2, verdict({ outcome: "failed", stale: true }))).toContain("STALE");
  });
});
