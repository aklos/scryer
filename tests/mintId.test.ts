/**
 * Frontend id minting (`src/viewmodel.ts`): random suffixes, the backend's
 * twin. The sequential scheme collided the moment two branches or sessions
 * minted against the same snapshot; a random draw checked against every id
 * in sight does not.
 */
import { describe, expect, it } from "vitest";
import { addNode, mintId, nextNodeId, nextResponsibilityId } from "../src/viewmodel";
import type { ScryModel } from "../src/viewmodel";

const model = (ids: string[]): ScryModel =>
  ({
    nodes: ids.map((id) => ({ id, kind: "component", name: id, responsibilities: [] })),
    groups: [],
    links: [],
  }) as unknown as ScryModel;

describe("mintId", () => {
  it("draws <prefix>-<6 chars> from the unambiguous alphabet", () => {
    const id = mintId("node", []);
    expect(id).toMatch(/^node-[0-9a-hj-km-np-tv-z]{6}$/);
  });

  it("never returns an id already in sight, and two draws over the same snapshot differ", () => {
    const seen = new Set<string>();
    for (let i = 0; i < 200; i++) {
      const id = mintId("resp", seen);
      expect(seen.has(id)).toBe(false);
      seen.add(id);
    }
  });

  it("clears both the plan and the committed layer when adding a node", () => {
    const plan = model(["node-1"]);
    const committed = model(["node-1", "node-2"]);
    const id = nextNodeId(plan, committed);
    expect(id).not.toBe("node-2");
    expect(id).toMatch(/^node-[0-9a-z]{6}$/);
    const rid = nextResponsibilityId([{ id: "resp-1", statement: "" }], plan, committed);
    expect(rid).toMatch(/^resp-[0-9a-z]{6}$/);
    expect(rid).not.toBe("resp-1");
  });

  it("addNode mints a random id unknown to both layers", () => {
    const plan = model(["node-1"]);
    const committed = model(["node-1", "node-2"]);
    const { model: next, id } = addNode(
      plan,
      { kind: "component", name: "Fresh", parentId: "node-1" },
      committed,
    );
    expect(id).toMatch(/^node-[0-9a-z]{6}$/);
    expect(next.nodes.some((n) => n.id === id)).toBe(true);
    expect(id).not.toBe("node-2");
  });
});
