import { describe, expect, it } from "vitest";
import { firstLine, inheritedDecisions } from "../src/widgets/node-page-sections/DecisionsSection";
import type { Node, ScryModel } from "../src/entities/model/viewmodel";

const n = (id: string, parentId?: string, decisions?: string): Node =>
  ({ id, kind: "component", name: id.toUpperCase(), parentId, decisions }) as Node;

describe("DecisionsSection", () => {
  it("Lists the decisions a node inherits from the nodes around it, nearest first", () => {
    const model = {
      nodes: [n("sys", undefined, "One store."), n("app", "sys"), n("ui", "app", "- Grid, not force."), n("tree", "ui")],
    } as ScryModel;
    const tree = model.nodes[3];
    expect(inheritedDecisions(model, tree).map((d) => d.name)).toEqual(["UI", "SYS"]);
    expect(firstLine(inheritedDecisions(model, tree)[0].decisions)).toBe("Grid, not force.");
  });
});
