/** Claims for agents driving the view: which move a direction plays, and what it tells the developer. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DirectionNotice, linkIsFor, movement, noticeFor, placeOf, type Direction, type Place } from "../src/features/agent-direction";
import { emptyModel, type ScryModel } from "../src/entities/model/viewmodel";

// scryer ─┬─ app ─┬─ canvas
//         │       └─ sessions ── follow
//         └─ core ── storage
const model: ScryModel = (() => {
  const m = emptyModel();
  m.nodes = [
    { id: "scryer", kind: "system", name: "scryer" },
    { id: "app", kind: "container", name: "Desktop app", parentId: "scryer" },
    { id: "canvas", kind: "component", name: "diagram-canvas", parentId: "app" },
    { id: "sessions", kind: "component", name: "sessions", parentId: "app" },
    { id: "follow", kind: "symbol", name: "useFollow", parentId: "sessions" },
    { id: "core", kind: "container", name: "scryer-core", parentId: "scryer" },
    { id: "storage", kind: "component", name: "storage", parentId: "core" },
  ];
  return m;
})();

const map = (focus: string | null): Place => ({ view: "diagram", focus, page: null });
const wiki = (page: string | null): Place => ({ view: "wiki", focus: null, page });
const direction = (over: Partial<Direction> = {}): Direction => ({ sender: "fix html export", node: "sessions", at: 5, ...over });

describe("movement", () => {
  it("When the target sits inside a card on the current map level, zooms in through that card", () => {
    expect(movement(model, map("scryer"), placeOf(model, "sessions", "diagram"))).toEqual({ kind: "push", via: "app" });
    expect(movement(model, map(null), placeOf(model, "follow", "diagram"))).toEqual({ kind: "push", via: "scryer" });
  });

  it("When the current map level sits inside the target's level, zooms out to the card that held it", () => {
    expect(movement(model, map("sessions"), placeOf(model, "core", "diagram"))).toEqual({ kind: "pull", via: "app" });
  });

  it("When neither map level holds the other, hops; when the target is on the current level, stays", () => {
    expect(movement(model, map("app"), placeOf(model, "storage", "diagram"))).toEqual({ kind: "hop" });
    expect(movement(model, map("app"), placeOf(model, "canvas", "diagram"))).toEqual({ kind: "none" });
  });

  it("When the wiki goes to a node inside the open page's node, pushes; to one above it, pulls; elsewhere, hops", () => {
    expect(movement(model, wiki("app"), placeOf(model, "sessions", "wiki")).kind).toBe("push");
    expect(movement(model, wiki("follow"), placeOf(model, "app", "wiki")).kind).toBe("pull");
    expect(movement(model, wiki("sessions"), placeOf(model, "storage", "wiki")).kind).toBe("hop");
    expect(movement(model, wiki(null), placeOf(model, "storage", "wiki")).kind).toBe("hop");
    expect(movement(model, wiki("storage"), placeOf(model, "storage", "wiki")).kind).toBe("none");
  });

  it("When the direction asks for the other view, morphs between the node's card and its page", () => {
    expect(movement(model, map("app"), placeOf(model, "sessions", "wiki"))).toEqual({ kind: "morph", via: "sessions" });
    expect(movement(model, map("scryer"), placeOf(model, "follow", "wiki"))).toEqual({ kind: "morph", via: "app" });
    expect(movement(model, map("core"), placeOf(model, "sessions", "wiki"))).toEqual({ kind: "hop" });
    expect(movement(model, wiki("storage"), placeOf(model, "sessions", "diagram"))).toEqual({ kind: "morph", via: "sessions" });
  });
});

describe("useDirections", () => {
  const has = (id: string) => model.nodes.some((n) => n.id === id);

  it("A direction always raises a notice to accept; it never moves the view by itself", () => {
    expect(noticeFor(direction(), { hasNode: has, lastAt: 0 })).toEqual({ direction: direction(), moved: false });
  });

  it("If a direction names a node the tree lacks, or repeats the last one, then raises nothing", () => {
    expect(noticeFor(direction({ node: "gone" }), { hasNode: has, lastAt: 0 })).toBeNull();
    expect(noticeFor(direction(), { hasNode: has, lastAt: 5 })).toBeNull();
  });
});

describe("useLinks", () => {
  it("When a clicked link points into another project than the one open, opens that project before following it", () => {
    const link = { project: "/home/a/notes/", direction: direction() };
    expect(linkIsFor(link, "/home/a/notes")).toBe(true);
    expect(linkIsFor(link, "/home/a/other")).toBe(false);
    expect(linkIsFor(link, null)).toBe(false);
  });
});

describe("DirectionNotice", () => {
  const draw = (moved: boolean, note?: string) =>
    renderToStaticMarkup(
      <DirectionNotice
        notice={{ direction: direction({ note }), moved }}
        nodeName={(id) => model.nodes.find((n) => n.id === id)!.name}
        onGo={() => {}}
        onDismiss={() => {}}
      />,
    );

  it("Names the session that sent the direction and where it points, with the agent's note", () => {
    const html = draw(true, "the claim I just proved");
    expect(html).toContain("fix html export");
    expect(html).toContain("showed you");
    expect(html).toContain("sessions");
    expect(html).toContain("the claim I just proved");
  });

  it("When the view didn't move, offers a link that goes there", () => {
    expect(draw(false)).toContain(">Show<");
    expect(draw(false)).toContain("points to");
    expect(draw(true)).not.toContain(">Show<");
  });
});
