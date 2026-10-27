import { describe, expect, it } from "vitest";
import {
  followTarget,
  liveFirst,
  sessionCounts,
  sessionTitle,
  stepLine,
  type LoggedSession,
  type SessionStep,
} from "../src/features/sessions/useSessions";
import { followView, stepView } from "../src/features/sessions/followView";
import { emptyModel, type ScryModel } from "../src/entities/model/viewmodel";

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
const edit = (at: number, file: string, nodeId?: string, edits = 1): SessionStep => ({ kind: "edit", at, file, nodeId, edits });

describe("useSessions", () => {
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

  it("Follows a session to the node of its latest claim step or edit in a component, and nowhere before it has one", () => {
    expect(followTarget(session())).toBeNull();
    expect(followTarget(session({ steps: [edit(1, "README.md")] }))).toBeNull();
    expect(
      followTarget(session({ steps: [edit(1, "src/a.ts", "a"), edit(2, "src/b.ts", "b"), edit(3, "README.md"), { kind: "stop", at: 4 }] })),
    ).toBe("b");
    expect(followTarget(session({ steps: [edit(1, "src/a.ts", "a"), { kind: "verdict", at: 2, claim }] }))).toBe("n");
  });

  it("Tells each step of a session's feed as one line, naming the component an edit falls in and how many edits ran together", () => {
    const name = (id: string) => (id === "a" ? "drafts" : id);
    expect(stepLine(edit(1, "src/a.ts", "a", 3), name)).toBe("edited src/a.ts in drafts ×3");
    expect(stepLine(edit(1, "README.md"), name)).toBe("edited README.md");
    expect(stepLine({ kind: "todo", at: 1, claim: { ...claim, state: "planned" } }, name)).toBe(
      "planned: Rejects a refund larger than the capture",
    );
    expect(stepLine({ kind: "verdict", at: 1, claim }, name)).toBe("proved: Rejects a refund larger than the capture");
    expect(stepLine({ kind: "verdict", at: 1, claim: { ...claim, state: "failing" } }, name)).toMatch(/^failed: /);
    expect(stepLine({ kind: "stop", at: 1 }, name)).toBe("finished a turn");
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

const tree = (over: (m: ScryModel) => void = () => {}): ScryModel => {
  const m = emptyModel();
  m.nodes = [
    { id: "c", kind: "container", name: "app" },
    { id: "a", kind: "component", name: "drafts", parentId: "c", description: "Saves drafts." },
    { id: "b", kind: "component", name: "sync", parentId: "c" },
  ];
  over(m);
  return m;
};

describe("followView", () => {
  it("When a followed change adds, moves, links or unlinks the node, shows it on the map", () => {
    expect(followView(tree((m) => (m.nodes = m.nodes.filter((n) => n.id !== "a"))), tree(), "a")).toBe("diagram");
    expect(followView(tree(), tree((m) => (m.nodes[1].parentId = undefined)), "a")).toBe("diagram");
    expect(followView(tree(), tree((m) => m.links.push({ id: "l", src: "a", dst: "b", label: "" })), "a")).toBe("diagram");
  });

  it("When a followed change rewrites the node's claims or description, shows it in the wiki", () => {
    expect(followView(tree(), tree((m) => (m.nodes[1].description = "Queues drafts.")), "a")).toBe("wiki");
    expect(followView(tree(), tree((m) => (m.nodes[1].responsibilities = [{ id: "r", statement: "Saves offline" }])), "a")).toBe("wiki");
    expect(stepView({ kind: "verdict", at: 1, claim })).toBe("wiki");
    expect(stepView({ kind: "todo", at: 1, claim })).toBe("wiki");
  });

  it("If nothing changed at the followed node, then keeps the view, whatever changed elsewhere", () => {
    expect(followView(tree(), tree((m) => (m.nodes[2].description = "Syncs.")), "a")).toBeNull();
    expect(followView(tree(), tree(), "a")).toBeNull();
    expect(stepView(edit(1, "src/a.ts", "a"))).toBeNull();
  });
});

describe("useFollow", () => {
  it.todo("When you select a node other than where the followed session went, stops following");
  it.todo("When the followed session stops being live, stops following and says so");
});
