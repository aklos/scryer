/**
 * The claim row's test lane: tone and words for a recorded verdict
 * (`src/health.ts`). A current green verdict is its OWN tone (`passing`) so
 * the tooltip never conflates it with the unmeasured (`quiet`), but neither
 * draws a glyph: passing is assumed, and marks after the count are for what
 * is wrong — stale outranks failing, because a red verdict the code moved
 * past is outdated, not an alarm — plus the one earned positive, probed.
 */
import { describe, expect, it } from "vitest";
import {
  probeMark,
  probeTitle,
  subtreeTestTone,
  testLaneGlyph,
  testLaneTitle,
  testLaneTone,
  type ClaimTestStatus,
  type ClaimProbeStatus,
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
  it("reads a current green verdict as passing, and only the unmeasured as quiet", () => {
    expect(testLaneTone(verdict({ outcome: "passed" }))).toBe("passing");
    // No run recorded, and a test that ran but asserted nothing: unmeasured.
    expect(testLaneTone(undefined)).toBe("quiet");
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

  it("ranks the rollup failing over stale over passing over quiet", () => {
    expect(
      subtreeTestTone(model, "root", {
        "r-mid": verdict({ stale: true }),
        "r-leaf": verdict({ outcome: "failed" }),
      }),
    ).toBe("failing");
    // One stale verdict outranks its green neighbours — the subtree can't be
    // called green while part of it hasn't been measured against this code.
    expect(
      subtreeTestTone(model, "root", { "r-mid": verdict({ stale: true }), "r-leaf": verdict({}) }),
    ).toBe("stale");
    expect(subtreeTestTone(model, "root", { "r-mid": verdict({ stale: true }) })).toBe("stale");
  });

  it("reports passing on a green subtree, quiet only when nothing was recorded", () => {
    expect(subtreeTestTone(model, "root", { "r-leaf": verdict({}) })).toBe("passing");
    expect(subtreeTestTone(model, "root", {})).toBe("quiet");
    // Verdicts outside the scope neither green it nor alarm it.
    expect(subtreeTestTone(model, "mid", { "r-out": verdict({ outcome: "failed" }) })).toBe(
      "quiet",
    );
    expect(subtreeTestTone(model, "mid", { "r-out": verdict({}) })).toBe("quiet");
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

/**
 * The probe axis. Deliberately independent of the verdict: a claim can be
 * green and hollow at once, and that combination is the whole reason the mark
 * exists. Absence is load-bearing — unprobed and probed-clean must never
 * render alike, the same rule the verdict lane already follows for
 * never-run vs passing.
 */
const probe = (o: Partial<ClaimProbeStatus>): ClaimProbeStatus => ({
  respId: "r1",
  probes: 3,
  survived: 0,
  survivors: [],
  stale: false,
  recordedAt: 1,
  ...o,
});

describe("probeMark", () => {
  it("marks a surviving break as hollow, however green the verdict beside it", () => {
    expect(probeMark(probe({ survived: 1, survivors: ["returned 2"] }))).toBe("hollow");
  });

  it("marks a clean round as probed", () => {
    expect(probeMark(probe({}))).toBe("probed");
  });

  it("shows nothing for a claim nobody probed — absence is not clean", () => {
    expect(probeMark(undefined)).toBe("none");
    expect(probeMark(probe({ probes: 0 }))).toBe("none");
  });

  it("shows nothing for a stale result, which describes code that has moved on", () => {
    expect(probeMark(probe({ stale: true }))).toBe("none");
    expect(probeMark(probe({ survived: 2, stale: true }))).toBe("none");
  });
});

describe("probeTitle", () => {
  it("names every survivor, so the finding is readable without opening anything", () => {
    const t = probeTitle(probe({ survived: 1, survivors: ["returning 2 went unnoticed"] }));
    expect(t).toContain("UNCAUGHT");
    expect(t).toContain("returning 2 went unnoticed");
  });

  it("calls a clean round a sample, never a proof", () => {
    expect(probeTitle(probe({}))).toContain("A sample, not a proof");
  });

  it("says nothing when there is nothing measured", () => {
    expect(probeTitle(undefined)).toBe("");
  });
});

describe("testLaneGlyph", () => {
  const probe = (over: Partial<ClaimProbeStatus>): ClaimProbeStatus => ({
    respId: "r1",
    probes: 2,
    survived: 0,
    survivors: [],
    stale: false,
    recordedAt: 0,
    ...over,
  });

  it("gives passing no glyph — the resting state is not a mark", () => {
    expect(testLaneGlyph(verdict({}), undefined)).toBe("none");
    expect(testLaneGlyph(undefined, undefined)).toBe("none");
  });

  it("lets a probe result be the lane's one glyph, never a second icon beside a check", () => {
    expect(testLaneGlyph(verdict({}), probe({}))).toBe("probed");
    expect(testLaneGlyph(verdict({}), probe({ survived: 1, survivors: ["x"] }))).toBe("hollow");
  });

  it("keeps the verdict's own glyph when it is stale or failing, whatever the probe said", () => {
    expect(testLaneGlyph(verdict({ stale: true }), probe({}))).toBe("stale");
    expect(testLaneGlyph(verdict({ outcome: "failed" }), probe({ survived: 1 }))).toBe("failing");
    expect(testLaneGlyph(undefined, probe({}))).toBe("none");
  });

  it("drops a stale probe back to no glyph", () => {
    expect(testLaneGlyph(verdict({}), probe({ stale: true }))).toBe("none");
  });
});
