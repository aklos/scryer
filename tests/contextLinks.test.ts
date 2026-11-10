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

  it("When an external only calls into the system, places it above the system with the people", async () => {
    const withCaller = {
      ...model,
      nodes: [...model.nodes, { id: "agent", kind: "system", name: "agent", external: true }],
      links: [...model.links, { id: "l-a", src: "agent", dst: "sys", label: "calls" }],
    } as unknown as ScryModel;
    const scene = await buildDiagramScene(withCaller, null, report);
    const at = (id: string) => scene.nodes.find((n) => n.id === id)!;
    expect(at("agent").y).toBe(at("merchant").y);
    expect(at("agent").y).toBeLessThan(at("sys").y);
    expect(at("bank").x).toBeGreaterThan(at("sys").x);
  });

  it("Labels a link into a node outside the level as authored, and leaves one read off imports unlabelled", async () => {
    const withImports = {
      ...model,
      nodes: [...model.nodes, { id: "billing", kind: "system", name: "Billing" }],
    } as unknown as ScryModel;
    const imports = {
      ...report,
      derived: { resolvedEdges: [{ srcNode: "gateway", dstNode: "billing", count: 3 }] },
    } as unknown as ModelHealthReport;
    const scene = await buildDiagramScene(withImports, "sys", imports);
    expect(pairs(scene.edges)).toEqual(["gateway>billing:", "ledger>db:stores entries in"]);
  });

  it("When a declared link sits on a component, draws it on that component's container map to the other end at its level, labelled and never quiet", async () => {
    const nested = {
      ...model,
      nodes: [
        ...model.nodes,
        { id: "spawner", kind: "component", name: "spawner", parentId: "gateway" },
        { id: "router", kind: "component", name: "router", parentId: "gateway" },
        { id: "sidecar", kind: "container", name: "Sidecar", parentId: "sys" },
        { id: "server", kind: "component", name: "server", parentId: "sidecar" },
      ],
      links: [
        ...model.links,
        { id: "l-run", src: "spawner", dst: "sidecar", label: "runs as its sidecar", declared: true },
        { id: "l-imp", src: "router", dst: "spawner", label: "", kind: "depends" },
        { id: "l-why", src: "router", dst: "spawner", label: "hands requests to", declared: true },
        { id: "l-code", src: "router", dst: "server", label: "" },
      ],
    } as unknown as ScryModel;
    const scene = await buildDiagramScene(nested, "gateway", report);
    expect(pairs(scene.edges)).toEqual(["router>sidecar:", "router>spawner:hands requests to", "spawner>sidecar:runs as its sidecar"]);
    expect(scene.edges.filter((e) => e.declared).every((e) => !e.conforming)).toBe(true);
    expect(scene.nodes.find((n) => n.id === "sidecar")?.reference).toBe(true);
    expect(scene.nodes.some((n) => n.id === "server")).toBe(false);
  });
});
