import { describe, expect, it } from "vitest";
import {
  liveFirst,
  sessionCounts,
  sessionTitle,
  type LoggedSession,
} from "../src/features/sessions/useSessions";

const session = (over: Partial<LoggedSession> = {}): LoggedSession => ({
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
const claim = { respId: "r", nodeId: "n", statement: "Rejects a refund larger than the capture", file: "f", line: 1, state: "proven" as const };

describe("useSessions", () => {
  it("Sums a session up by the claims it moved — proven, failing, todo it wrote — leaving out what it did not move", () => {
    expect(sessionCounts(session())).toEqual([]);
    expect(
      sessionCounts(
        session({
          newlyProven: [claim, claim],
          newlyPlanned: [claim],
          stillPlanned: [claim],
          newlyStaleOrFailing: [{ ...claim, state: "stale" }, { ...claim, state: "failing" }],
          newDarkFiles: ["a.ts"],
        }),
      ),
    ).toEqual(["2 proven", "1 failing", "1 todo"]);
  });

  it("Names a session by its agent's task in quotes, else as a session in the first component it touched, else as a session on its branch", () => {
    expect(sessionTitle({ title: "Route edges", titledByTask: true, startedAt: 0 }, "main")).toBe("“Route edges”");
    expect(sessionTitle({ title: "drafts", titledByTask: false, startedAt: 0 }, "main")).toBe("session in drafts");
    expect(sessionTitle({ title: "", startedAt: 0 }, "main")).toMatch(/^session on main · /);
  });

  it("Lists live sessions first, each group newest first", () => {
    const order = liveFirst([
      session({ session: "old", endedAt: 1 }),
      session({ session: "live-old", endedAt: 2, live: true }),
      session({ session: "new", endedAt: 5 }),
      session({ session: "live-new", endedAt: 4, live: true }),
    ]).map((s) => s.session);
    expect(order).toEqual(["live-new", "live-old", "new", "old"]);
  });
});

describe("ActivityPage", () => {
  it("Says when a session ended as minutes or hours ago, else by its day", async () => {
    const { ago } = await import("../src/pages/landing/ActivityPage");
    expect(ago(1000, 1030)).toBe("just now");
    expect(ago(1000, 1000 + 12 * 60)).toBe("12m ago");
    expect(ago(1000, 1000 + 3 * 3600)).toBe("3h ago");
  });
});
