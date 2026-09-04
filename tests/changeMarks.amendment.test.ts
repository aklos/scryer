/**
 * The amendment mark (`P`, `src/changeMarks.ts`): a claim the agent reworded or
 * added AFTER the developer signed off its change is vagrant with an origin,
 * and the tree must show it apart from a code-discovered vagrant (`Q`) — "what
 * the agent changed" vs "what the code does that I never said".
 */
import { describe, expect, it } from "vitest";
import {
  CHANGE_COLOR,
  driftOf,
  groupDrift,
  MARK_KIND,
  MARK_META,
  nodeDrift,
  rollupMarks,
} from "../src/changeMarks";
import type { Group, Node, ScryModel } from "../src/viewmodel";

const node = (id: string, extra: Partial<Node> = {}): Node =>
  ({ id, kind: "component", name: id, ...extra }) as Node;

describe("amendment mark", () => {
  it("marks a vagrantOrigin claim P, and P outranks a code-discovered Q", () => {
    const amendedOnly = node("a", {
      responsibilities: [
        { id: "r1", statement: "amended", vagrant: true, vagrantOrigin: "amendment", approvedStatement: "approved" },
      ],
    });
    expect(nodeDrift(amendedOnly)).toBe("P");

    const added = node("b", {
      responsibilities: [{ id: "r2", statement: "added", vagrant: true, vagrantOrigin: "addition" }],
    });
    expect(nodeDrift(added)).toBe("P");

    const both = node("c", {
      vagrant: true,
      responsibilities: [
        { id: "q", statement: "found in code", vagrant: true },
        { id: "p", statement: "amended", vagrant: true, vagrantOrigin: "amendment" },
      ],
      properties: [{ label: "field", vagrant: true }],
    });
    expect(nodeDrift(both)).toBe("P");

    const plainVagrant = node("d", {
      responsibilities: [{ id: "q", statement: "found in code", vagrant: true }],
    });
    expect(nodeDrift(plainVagrant)).toBe("Q");
  });

  it("does the same for group-held claims", () => {
    const g: Group = {
      id: "g",
      name: "g",
      memberIds: [],
      responsibilities: [
        { id: "q", statement: "found", vagrant: true },
        { id: "p", statement: "amended", vagrant: true, vagrantOrigin: "amendment" },
        { id: "x", statement: "stale", stale: true },
      ],
    };
    expect(groupDrift(g)).toBe("P");
    expect(driftOf([{ vagrant: true }, { stale: true }])).toBe("Q");
    expect(driftOf([{ stale: true }])).toBe("X");
    expect(driftOf([])).toBeNull();
  });

  it("is its own violet category with the sign-off label", () => {
    expect(MARK_KIND.P).toBe("amendment");
    expect(CHANGE_COLOR.amendment).toBe("text-violet-700 dark:text-violet-400");
    expect(MARK_META.P.color).toBe(CHANGE_COLOR.amendment);
    expect(MARK_META.P.label).toBe("Changed after sign-off (awaiting verdict)");
  });

  it("rolls P up over Q and X onto the ancestor branch", () => {
    const model: ScryModel = {
      version: "0.3",
      nodes: [
        node("root", { kind: "container" }),
        node("q", { parentId: "root", responsibilities: [{ id: "rq", statement: "q", vagrant: true }] }),
        node("x", { parentId: "root", responsibilities: [{ id: "rx", statement: "x", stale: true }] }),
        node("p", {
          parentId: "root",
          responsibilities: [{ id: "rp", statement: "p", vagrant: true, vagrantOrigin: "addition" }],
        }),
      ],
      links: [],
      groups: [],
    };
    const rolled = rollupMarks(model, null, []);
    expect(rolled.get("root")).toEqual({ plan: null, drift: "P" });
  });
});
