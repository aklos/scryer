/** Claims for `modelChanges` and `alignClaims`. */
import { describe, expect, it } from "vitest";
import { alignClaims, modelChanges } from "../src/entities/model/changes";
import type { Node, ScryModel } from "../src/entities/model/viewmodel";

const model = (nodes: Partial<Node>[], extra: Partial<ScryModel> = {}): ScryModel =>
  ({
    version: 1,
    nodes: nodes.map((n) => ({ kind: "component", name: n.id, ...n })),
    links: [],
    ...extra,
  }) as unknown as ScryModel;

const tree = [
  { id: "sys", kind: "system" as const },
  { id: "app", kind: "container" as const, parentId: "sys" },
  { id: "notes", parentId: "app", claims: [{ id: "c1", statement: "Saves a note" }] },
  { id: "tags", parentId: "app" },
];

describe("modelChanges", () => {
  it("Marks a node the base lacks as added and one whose name, description, claims or outgoing links differ as modified", () => {
    const base = model(tree);
    const next = model(
      [
        ...tree.map((n) => (n.id === "notes" ? { ...n, claims: [{ id: "c1", statement: "Saves a draft" }] } : n)),
        { id: "search", parentId: "app" },
      ],
      { links: [{ id: "l", src: "tags", dst: "notes", label: "reads" }] },
    );
    const { nodes } = modelChanges(base, next);
    expect(Object.fromEntries(nodes)).toEqual({ notes: "modified", search: "added", tags: "modified" });
    expect(modelChanges(base, model(tree)).nodes.size).toBe(0);
  });

  it("Marks every ancestor of a changed or removed node as holding changes, and counts the removed", () => {
    const base = model(tree);
    const next = model(tree.filter((n) => n.id !== "tags"));
    const changes = modelChanges(base, next);
    expect(changes.removed).toBe(1);
    expect([...changes.holding].sort()).toEqual(["app", "sys"]);
    expect(changes.nodes.size).toBe(0);
  });

  it("Reads no changes without a base", () => {
    expect(modelChanges(null, model(tree)).nodes.size).toBe(0);
  });
});

describe("alignClaims", () => {
  it("When a claim's id changed but it is still the same test, pairs it with its base copy as reworded", () => {
    const base = [
      { id: "old", statement: "Saves a note" },
      { id: "gone", statement: "Deletes a note" },
    ];
    const current = [
      { id: "new", statement: "Saves a draft" },
      { id: "moved", statement: "Deletes a note" },
    ];
    const baseTests = {
      old: [{ pattern: "notes.test.ts", symbol: "Saves a note", line: 4 }],
      gone: [{ pattern: "notes.test.ts", symbol: "Deletes a note", line: 9 }],
    };
    const tests = {
      new: [{ pattern: "notes.test.ts", symbol: "Saves a draft", line: 4 }],
      moved: [{ pattern: "other.test.ts", symbol: "Deletes a note", line: 9 }],
    };
    expect(alignClaims(base, current, baseTests, tests).map((c) => c.id)).toEqual(["new", "gone"]);
  });
});
