import { describe, expect, it } from "vitest";
import { sessionCounts, sessionTitle } from "../src/pages/landing/LandingPage";

const session = (over = {}) => ({
  session: "s1",
  title: "t",
  startedAt: 0,
  endedAt: 0,
  newlyProven: [],
  newlyPlanned: [],
  stillPlanned: [],
  newlyStaleOrFailing: [],
  survivingMutants: 0,
  newDarkFiles: [],
  components: [],
  ...over,
});
const claim = { respId: "r", nodeId: "n", statement: "s", file: "f", line: 1, state: "proven" as const };

describe("LandingPage", () => {
  it("Sums a session up on one line by what changed state, leaving out kinds where nothing did", () => {
    expect(sessionCounts(session())).toEqual([]);
    expect(
      sessionCounts(session({ newlyProven: [claim, claim], newlyPlanned: [claim], stillPlanned: [claim], newDarkFiles: ["a.ts"] })),
    ).toEqual(["+2 proven", "2 todo", "1 new dark file"]);
  });

  it("Names a session by its agent's task in quotes, else as a session in the first component it touched, else as a session on its branch", () => {
    expect(sessionTitle({ title: "Route edges", titledByTask: true, startedAt: 0 }, "main")).toBe("“Route edges”");
    expect(sessionTitle({ title: "drafts", titledByTask: false, startedAt: 0 }, "main")).toBe("session in drafts");
    expect(sessionTitle({ title: "", startedAt: 0 }, "main")).toMatch(/^session on main · /);
  });
});
