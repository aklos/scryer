/**
 * The test dimension's "what needs a look" list (`src/health.ts`): untested
 * committed claims, failing verdicts, and probes that let a break through —
 * never passing claims or stale verdicts — and its per-subtree rollup, which
 * the tree's Tests lens reads.
 */
import { describe, expect, it } from "vitest";
import {
  rollupTestFindings,
  testFindings,
  type ClaimProbeStatus,
  type ClaimTestStatus,
} from "../src/health";
import type { Node, ScryModel } from "../src/viewmodel";

const node = (id: string, parentId: string | undefined, over: Partial<Node> = {}): Node => ({
  id,
  kind: "component",
  name: id,
  parentId,
  responsibilities: [],
  ...over,
});
const model = (nodes: Node[], testMap: ScryModel["testMap"] = {}): ScryModel =>
  ({ nodes, groups: [], links: [], testMap }) as unknown as ScryModel;
const verdict = (over: Partial<ClaimTestStatus>): ClaimTestStatus => ({
  respId: "r",
  outcome: "passed",
  cases: 1,
  stale: false,
  recordedAt: 0,
  ...over,
});
const probe = (over: Partial<ClaimProbeStatus>): ClaimProbeStatus => ({
  respId: "r",
  probes: 1,
  survived: 0,
  survivors: [],
  stale: false,
  recordedAt: 0,
  ...over,
});

const testable = { id: "r1", statement: "**When** a save fails, **retry** once" };
const ubiquitous = { id: "r2", statement: "**Persist** every event" };

describe("testFindings", () => {
  it("lists a committed testable claim with no test as untested, and nothing for a ubiquitous or plan-only one", () => {
    const leaf = node("leaf", undefined, {
      responsibilities: [testable, ubiquitous, { id: "r3", statement: "**If** x, **then** y" }],
    });
    const committed = model([node("leaf", undefined, { responsibilities: [testable, ubiquitous] })]);
    const out = testFindings(model([leaf]), committed, {}, {});
    expect(out.map((f) => [f.kind, f.resp.id])).toEqual([["untested", "r1"]]);
  });

  it("never nags an external or person host for a missing test", () => {
    const ext = node("ext", undefined, { external: true, responsibilities: [testable] });
    expect(testFindings(model([ext]), model([ext]), {}, {})).toEqual([]);
  });

  it("reports a current failing verdict, and a probe survivor, but not passing or stale", () => {
    const leaf = node("leaf", undefined, {
      responsibilities: [
        { id: "a", statement: "**When** a, **do** a" },
        { id: "b", statement: "**When** b, **do** b" },
        { id: "c", statement: "**When** c, **do** c" },
        { id: "d", statement: "**When** d, **do** d" },
      ],
    });
    const tests = { a: [{ pattern: "t" }], b: [{ pattern: "t" }], c: [{ pattern: "t" }], d: [{ pattern: "t" }] };
    const m = model([leaf], tests);
    const out = testFindings(
      m,
      m,
      {
        a: verdict({ outcome: "failed" }),
        b: verdict({ outcome: "passed" }),
        c: verdict({ outcome: "failed", stale: true }),
        d: verdict({ outcome: "passed" }),
      },
      { d: probe({ survived: 1, survivors: ["flipped the guard"] }) },
    );
    expect(out.map((f) => [f.kind, f.resp.id])).toEqual([
      ["failing", "a"],
      ["hollow", "d"],
    ]);
    expect(out[1].survivors).toEqual(["flipped the guard"]);
  });
});

describe("rollupTestFindings", () => {
  it("counts each finding on its host and every ancestor, leaving clean branches absent", () => {
    const nodes = [
      node("root", undefined),
      node("mid", "root"),
      node("leaf", "mid", { responsibilities: [testable] }),
      node("other", "root"),
    ];
    const m = model(nodes);
    const tally = rollupTestFindings(m, testFindings(m, m, {}, {}));
    expect(tally.get("leaf")).toEqual({ untested: 1, failing: 0, hollow: 0 });
    expect(tally.get("mid")?.untested).toBe(1);
    expect(tally.get("root")?.untested).toBe(1);
    expect(tally.has("other")).toBe(false);
  });
});
