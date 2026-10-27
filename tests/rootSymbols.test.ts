/**
 * A level with components is drawn at their altitude, as the tree reads it:
 * the root module's own symbols beside them are code, never cards on the map.
 */

import { describe, expect, it } from "vitest";
import type { ModelHealthReport, StyleViolation } from "../src/entities/model/health";
import { buildDiagramScene } from "../src/widgets/diagram-canvas/diagramLayout";
import type { ScryModel } from "../src/entities/model/viewmodel";

const model = {
  version: 1,
  nodes: [
    { id: "sys", kind: "system", name: "Sys" },
    { id: "ledger", kind: "container", name: "Ledger", parentId: "sys", style: "hexagonal" },
    { id: "app", kind: "component", name: "application", parentId: "ledger", layer: "application" },
    { id: "dom", kind: "component", name: "domain", parentId: "ledger", layer: "domain" },
    { id: "open", kind: "symbol", name: "open_in_memory", parentId: "ledger" },
    { id: "journal", kind: "symbol", name: "open_journaled", parentId: "ledger" },
    { id: "pay", kind: "container", name: "Payouts", parentId: "sys" },
  ],
  links: [
    { id: "l-root", src: "open", dst: "app", label: "", kind: "depends" },
    { id: "l-out", src: "journal", dst: "pay", label: "", kind: "depends" },
    { id: "l-bad", src: "dom", dst: "app", label: "", kind: "depends" },
  ],
  groups: [],
} as unknown as ScryModel;

const violation: StyleViolation = {
  kind: "forbidden_link",
  node: "dom",
  other: "app",
  file: "ledger/src/domain/memory.rs",
  container: "ledger",
  detail: "declared link l-bad: 'domain' → 'application' is illegal in style 'hexagonal'",
};
const report = {
  health: { disconnected: [] },
  anchors: [],
  derived: { resolvedEdges: [{ srcNode: "open", dstNode: "app", count: 2 }] },
  styles: [],
  structural: { violations: [violation] },
} as unknown as ModelHealthReport;

describe("root symbols beside components", () => {
  it("When the root defines symbols beside the components, draws only the components, in the style's drawing", async () => {
    const scene = await buildDiagramScene(model, "ledger", report);
    expect(scene.mode).toBe("styled");
    expect(scene.styled?.drawing).toBe("hexagon");
    // Not even as a ghost: an import from the root rolls up to the container itself.
    expect(scene.nodes.map((n) => n.id).sort()).toEqual(["app", "dom"]);
    expect(scene.edges.some((e) => [e.source, e.target].some((id) => id === "open" || id === "journal"))).toBe(false);
    expect(scene.edges.find((e) => e.source === "dom" && e.target === "app")?.violations).toEqual([violation.detail]);
  });

  it("When an unstyled level's root defines symbols beside the components, draws only the components", async () => {
    const plain = {
      ...model,
      nodes: model.nodes.map((n) => (n.id === "ledger" ? { ...n, style: undefined } : n)),
    } as unknown as ScryModel;
    const scene = await buildDiagramScene(plain, "ledger", null);
    expect(scene.mode).toBe("arch");
    expect(scene.nodes.map((n) => n.id).sort()).toEqual(["app", "dom"]);
  });

  it("Counts only the components on the container's card, and its root symbols' links leave the system level as they were", async () => {
    const scene = await buildDiagramScene(model, "sys", report);
    expect(scene.nodes.find((n) => n.id === "ledger")?.childIds.sort()).toEqual(["app", "dom"]);
    expect(scene.edges.some((e) => e.source === "ledger" && e.target === "pay")).toBe(true);
  });
});
