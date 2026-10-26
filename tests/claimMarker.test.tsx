import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ClaimMarker, ConcernTag } from "../src/widgets/node-page-sections/ResponsibilitiesSection";
import { CLAIM_TONE } from "../src/entities/model/claimState";
import {
  claimMark,
  claimMarkLabel,
  type ClaimProbeStatus,
  type ClaimTestStatus,
} from "../src/entities/model/health";

const verdict = (over: Partial<ClaimTestStatus> = {}): ClaimTestStatus => ({
  respId: "r1",
  outcome: "passed",
  cases: 1,
  stale: false,
  recordedAt: 0,
  ...over,
});

const probe = (over: Partial<ClaimProbeStatus> = {}): ClaimProbeStatus => ({
  respId: "r1",
  probes: 2,
  survived: 0,
  survivors: [],
  stale: false,
  recordedAt: 0,
  ...over,
});

const tone = (s: keyof typeof CLAIM_TONE) => CLAIM_TONE[s].split(" ")[0];

describe("claim row marker", () => {
  it("Marks a claim in front with its state's icon in its state's hue, named for the state", () => {
    expect(claimMark("proven", verdict(), undefined)).toBe("proven");
    expect(claimMark("failing", verdict({ outcome: "failed" }), undefined)).toBe("failing");
    expect(claimMark("planned", undefined, undefined)).toBe("todo");
    const html = renderToStaticMarkup(<ClaimMarker mark="failing" label="failing" />);
    expect(html).toContain("<svg");
    expect(html).toContain(tone("failing"));
    expect(html).toContain('aria-label="failing"');
    expect(renderToStaticMarkup(<ClaimMarker mark="todo" label="todo" />)).toContain(tone("planned"));
  });

  it("When every deliberate break was caught, marks a proven claim probed", () => {
    expect(claimMark("proven", verdict(), probe())).toBe("probed");
    expect(claimMark("proven", verdict(), probe({ stale: true }))).toBe("proven");
    expect(renderToStaticMarkup(<ClaimMarker mark="probed" />)).toContain(tone("proven"));
  });

  it("If a deliberate break survived, marks a proven claim hollow in the failing hue", () => {
    expect(claimMark("proven", verdict(), probe({ survived: 1, survivors: ["x"] }))).toBe("hollow");
    // A failing or changed claim keeps its own mark whatever the probe said.
    expect(claimMark("failing", verdict({ outcome: "failed" }), probe({ survived: 1 }))).toBe("failing");
    expect(renderToStaticMarkup(<ClaimMarker mark="hollow" />)).toContain(tone("failing"));
  });

  it("If a claim's test or code changed since it passed, names it changed since it passed", () => {
    const v = verdict({ stale: true });
    const mark = claimMark("stale", v, probe());
    expect(mark).toBe("changed");
    expect(claimMarkLabel(mark, v)).toBe("changed since it passed: re-run");
    expect(claimMarkLabel(mark, verdict({ stale: true, outcome: "failed" }))).toBe("changed since it last ran: re-run");
    expect(renderToStaticMarkup(<ClaimMarker mark="changed" />)).toContain(tone("stale"));
  });

  it("If a claim's test was reworded since it passed, names it reworded, though it never ran as worded", () => {
    const mark = claimMark("stale", undefined, undefined, true);
    expect(mark).toBe("changed");
    expect(claimMarkLabel(mark, undefined, true)).toBe("reworded since it passed: re-run");
  });

  it("If a claim never ran, names it not run yet", () => {
    const mark = claimMark("stale", undefined, undefined);
    expect(mark).toBe("unrun");
    expect(claimMarkLabel(mark, undefined)).toBe("not run yet");
  });
});

describe("claim row concern", () => {
  it("Shows a tagged claim's concern as its glyph and name, in no state's hue", () => {
    const html = renderToStaticMarkup(<ConcernTag slug="auth" concerns={[]} />);
    expect(html).toContain("<svg");
    expect(html).toContain(">auth<");
    for (const s of ["proven", "stale", "failing", "planned"] as const) expect(html).not.toContain(tone(s));
  });

  it("Shows nothing in the concern lane for an untagged claim", () => {
    expect(renderToStaticMarkup(<ConcernTag concerns={[]} />)).toBe("");
  });
});
