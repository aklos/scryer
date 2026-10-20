/**
 * Declared context relationships from structure.toml: drawn between persons,
 * the system and externals on the context map, and a container's use of an
 * external ghosts that external on the container map.
 */

import { describe, expect, it } from "vitest";
import type { ModelHealthReport } from "../src/entities/model/health";
import { buildDiagramScene } from "../src/widgets/diagram-canvas/diagramLayout";
import type { ScryModel } from "../src/entities/model/viewmodel";

const model = {
  version: 1,
  nodes: [
    { id: "sys", kind: "system", name: "Pay" },
    { id: "merchant", kind: "person", name: "merchant" },
    { id: "bank", kind: "system", name: "bank", external: true },
    { id: "db", kind: "system", name: "db", external: true },
    { id: "ledger", kind: "container", name: "Ledger", parentId: "sys" },
    { id: "gateway", kind: "container", name: "Gateway", parentId: "sys" },
  ],
  links: [
    { id: "l-m", src: "merchant", dst: "sys", label: "integrates" },
    { id: "l-b", src: "sys", dst: "bank", label: "" },
    { id: "l-u", src: "ledger", dst: "db", label: "stores entries in" },
    { id: "l-s", src: "sys", dst: "db", label: "stores entries in" },
  ],
  groups: [],
} as unknown as ScryModel;

const report = {
  health: { disconnected: [] },
  anchors: [],
  derived: { resolvedEdges: [] },
  styles: [],
  structural: { violations: [] },
} as unknown as ModelHealthReport;

const pairs = (edges: { source: string; target: string; label: string }[]) =>
  edges.map((e) => `${e.source}>${e.target}:${e.label}`).sort();

describe("context relationships", () => {
  it("When structure.toml relates persons, the system and externals, draws them on the context map", async () => {
    const scene = await buildDiagramScene(model, null, report);
    expect(pairs(scene.edges)).toEqual(["merchant>sys:integrates", "sys>bank:", "sys>db:stores entries in"]);
  });

  it("When a container uses an external, ghosts the external beside it on the container map", async () => {
    const scene = await buildDiagramScene(model, "sys", report);
    expect(pairs(scene.edges)).toEqual(["ledger>db:stores entries in"]);
    expect(scene.nodes.find((n) => n.id === "db")?.reference).toBe(true);
    expect(scene.nodes.some((n) => n.id === "bank" || n.id === "merchant")).toBe(false);
  });
});
