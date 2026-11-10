import { describe, expect, it } from "vitest";
import { levelFacts, nodeFacts } from "../src/widgets/diagram-canvas/levelFacts";

type Args = Parameters<typeof levelFacts>;
const model = {
  nodes: [
    { id: "sys", kind: "system", name: "Scryer" },
    { id: "host", kind: "container", name: "Desktop App Host", parentId: "sys", technology: "Rust" },
    { id: "demo", kind: "container", name: "Demo", parentId: "sys", tooling: true },
    { id: "core", kind: "container", name: "Model Core", parentId: "sys" },
  ],
} as unknown as Args[0];
const styled = {
  nodes: [{ reference: false }, { reference: false }, { reference: true }],
  edges: [{ violations: ["shell imports core wrongly"] }, {}, { violations: [] }],
  styled: { name: "core-shell", layers: ["shell", "core"], purpose: "CLIs, small libraries, config and script repos" },
} as unknown as Args[2];

describe("levelFacts", () => {
  it("When a container is held to a style, names it with its layers outermost first and what it is for, beside its kind and technology", () => {
    const f = levelFacts(model, "host", styled)!;
    expect([f.kind, f.technology]).toEqual(["Container", "Rust"]);
    expect(f.style).toEqual({ name: "core-shell", layers: ["shell", "core"], purpose: "CLIs, small libraries, config and script repos" });
    expect(f.unstyled).toBe(false);
  });

  it("Counts the cards inside the level, those outside it, and the links that break its style", () => {
    const f = levelFacts(model, "host", styled)!;
    expect([f.inside, f.insideWord, f.outside, f.violations]).toEqual([2, "components", 1, 1]);
  });

  it("If a container has no style declared, then says so, unless it is dev tooling, held to none", () => {
    const plain = { nodes: [{ reference: false }], edges: [] } as unknown as Args[2];
    expect(levelFacts(model, "core", plain)!.unstyled).toBe(true);
    const tooling = levelFacts(model, "demo", plain)!;
    expect([tooling.unstyled, tooling.tooling]).toEqual([false, true]);
  });

  it("Shows nothing at the top level, where the systems and people are the whole story", () => {
    expect(levelFacts(model, null, styled)).toBeNull();
  });
});

describe("nodeFacts", () => {
  const tree = {
    nodes: [
      { id: "host", kind: "container", name: "Desktop App Host" },
      { id: "core", kind: "container", name: "Model Core" },
      { id: "preview", kind: "component", name: "preview", parentId: "host" },
      { id: "state", kind: "component", name: "state", parentId: "host" },
    ],
  } as unknown as Args[0];
  const scene = {
    nodes: [
      { id: "preview", kind: "component", name: "preview", layer: "shell", technology: "Rust", reference: false, description: "Component previews.\n\nWhy: more detail." },
      { id: "state", kind: "component", name: "state", layer: "core", reference: false },
      { id: "core", kind: "container", name: "Model Core", reference: true },
    ],
    edges: [
      { source: "preview", target: "state" },
      { source: "preview", target: "core", violations: ["shell reaches out"] },
      { source: "state", target: "preview" },
    ],
    styled: { name: "core-shell", layers: ["shell", "core"], purpose: "", mayImport: { shell: ["core"], core: [] } },
  } as unknown as Args[2];

  it("When a card on the level is selected, gives its kind, technology, layer with what that layer may import, and its description's first paragraph", () => {
    const f = nodeFacts(tree, scene, "preview")!;
    expect([f.name, f.kind, f.technology]).toEqual(["preview", "Component", "Rust"]);
    expect(f.layer).toEqual({ name: "shell", mayImport: ["core"] });
    expect(f.summary).toBe("Component previews.");
  });

  it("Counts the arrows in and out of the selected card on this level, and those that break the style", () => {
    const f = nodeFacts(tree, scene, "preview")!;
    expect([f.arrowsIn, f.arrowsOut, f.violations]).toEqual([1, 2, 1]);
  });

  it("When the selected card is drawn from outside the level, says where it lives; with nothing selected here, shows nothing", () => {
    const outside = { ...tree, nodes: [...(tree.nodes as object[]), { id: "sys", kind: "system", name: "Scryer" }].map((n) => ((n as { id: string }).id === "core" ? { ...n, parentId: "sys" } : n)) } as unknown as Args[0];
    expect(nodeFacts(outside, scene, "core")!.livesIn).toBe("Scryer");
    expect(nodeFacts(tree, scene, null)).toBeNull();
    expect(nodeFacts(tree, scene, "elsewhere")).toBeNull();
  });
});
