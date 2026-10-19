/**
 * A styled level keeps its drawing when the container's root module defines
 * symbols of its own beside its components: they sit in a composition-root
 * strip under the drawing, with their links shown.
 */

import { describe, expect, it } from "vitest";
import type { ModelHealthReport, StyleViolation } from "../src/entities/model/health";
import { buildDiagramScene } from "../src/widgets/diagram-canvas/diagramLayout";
import { CARD_H } from "../src/widgets/diagram-canvas/styled";
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
  ],
  links: [
    { id: "l-root", src: "open", dst: "app", label: "", kind: "depends" },
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
  derived: { resolvedEdges: [] },
  styles: [],
  structural: { violations: [violation] },
} as unknown as ModelHealthReport;

describe("styled level with root symbols", () => {
  it("When the root defines symbols beside the components, keeps the style's drawing and puts them in a strip below it", async () => {
    const scene = await buildDiagramScene(model, "ledger", report);
    expect(scene.mode).toBe("styled");
    expect(scene.styled?.drawing).toBe("hexagon");
    const at = (id: string) => scene.nodes.find((n) => n.id === id)!;
    const lowestComponent = Math.max(at("app").y, at("dom").y);
    for (const id of ["open", "journal"]) expect(at(id).y).toBeGreaterThan(lowestComponent + CARD_H);
    expect(at("open").y).toBe(at("journal").y);
    expect(scene.regions?.some((r) => r.layer === "composition root")).toBe(true);
    // The root's link is drawn, not folded into the drawing; the violation stays red.
    expect(scene.edges.find((e) => e.source === "open" && e.target === "app")?.implied).toBe(false);
    expect(scene.edges.find((e) => e.source === "dom" && e.target === "app")?.violations).toEqual([violation.detail]);
  });
});
