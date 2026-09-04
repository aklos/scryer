import { describe, expect, it } from "vitest";
import { buildDiagramScene } from "../src/diagramLayout";
import type { ModelHealthReport } from "../src/health";
import type { ScryModel } from "../src/viewmodel";

// A system with two containers: one hexagonal (a domain component, an
// application component, and one with no layer), one declaring nothing.
const model = {
  version: "0.3",
  nodes: [
    { id: "sys", kind: "system", name: "Shop" },
    { id: "hex", kind: "container", name: "Ledger", parentId: "sys", style: "hexagonal" },
    { id: "plain", kind: "container", name: "Tooling", parentId: "sys" },
    { id: "dom", kind: "component", name: "Entries", parentId: "hex", layer: "domain" },
    { id: "app", kind: "component", name: "Posting", parentId: "hex", layer: "application" },
    { id: "lost", kind: "component", name: "Helpers", parentId: "hex" },
    { id: "tool", kind: "component", name: "Runner", parentId: "plain" },
  ],
  // domain → application is forbidden by the hexagonal matrix.
  links: [{ id: "l1", src: "dom", dst: "app", label: "", kind: "depends" }],
  groups: [],
  concerns: [],
} as unknown as ScryModel;

const report = {
  derived: { resolvedEdges: [] },
  completeness: {},
  style: {
    violations: [
      {
        kind: "layer_violation",
        node: "app",
        other: "dom",
        file: "src/posting.rs",
        container: "hex",
        detail: "Posting imports Entries the other way round",
      },
    ],
    counts: {},
  },
} as unknown as ModelHealthReport;

describe("conformance overlay facts", () => {
  it("marks the unstyled container and charges the styled one with its inside", async () => {
    const scene = await buildDiagramScene(model, "sys", report);
    const byId = new Map(scene.nodes.map((n) => [n.id, n]));
    expect(byId.get("plain")?.conformance).toEqual({ unstyled: true, layerless: false, violations: [] });
    expect(byId.get("hex")?.conformance?.violations).toEqual([
      "Posting imports Entries the other way round",
    ]);
  });

  it("names the layerless component and the forbidden declared link", async () => {
    const scene = await buildDiagramScene(model, "hex", report);
    const byId = new Map(scene.nodes.map((n) => [n.id, n]));
    expect(byId.get("lost")?.conformance?.layerless).toBe(true);
    const dom = byId.get("dom")?.conformance;
    expect(dom?.violations).toHaveLength(1);
    expect(dom?.violations[0]).toMatch(/domain/);
    // The report's violation lands on its source component, once.
    expect(byId.get("app")?.conformance?.violations).toEqual([
      "Posting imports Entries the other way round",
    ]);
  });

  it("leaves a clean styled component untagged", async () => {
    const clean = { ...model, links: [] } as ScryModel;
    const scene = await buildDiagramScene(clean, "hex", null);
    expect(scene.nodes.find((n) => n.id === "dom")?.conformance).toBeUndefined();
  });
});
