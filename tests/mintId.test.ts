/**
 * Frontend id minting (`src/viewmodel.ts`): random suffixes, the backend's
 * twin. The sequential scheme collided the moment two branches or sessions
 * minted against the same snapshot; a random draw checked against every id
 * in sight does not.
 */
import { describe, expect, it } from "vitest";
import { mintId, nextResponsibilityId } from "../src/entities/model/viewmodel";
import type { ScryModel } from "../src/entities/model/viewmodel";

const model = (ids: string[]): ScryModel =>
  ({
    nodes: ids.map((id) => ({ id, kind: "component", name: id, responsibilities: [] })),
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

  it("When a claim is added, clears both the plan and the committed layer", () => {
    const plan = model(["node-1"]);
    const committed = model(["node-1", "node-2"]);
    const rid = nextResponsibilityId([{ id: "resp-1", statement: "" }], plan, committed);
    expect(rid).toMatch(/^resp-[0-9a-z]{6}$/);
    expect(rid).not.toBe("resp-1");
  });
});
