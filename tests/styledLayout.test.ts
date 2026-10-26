import { describe, expect, it } from "vitest";
import { buildDiagramScene } from "../src/widgets/diagram-canvas/diagramLayout";
import type { ScryModel } from "../src/entities/model/viewmodel";
import type { ModelHealthReport } from "../src/entities/model/health";
import { builtinStyles } from "../src/entities/model/styles";
import { classifyStyledEdge, styledLayout, CARD_H, CARD_W } from "../src/widgets/diagram-canvas/styled";

const style = (name: string) => builtinStyles.find((s) => s.name === name)!;

describe("styledLayout", () => {
  it("rows: one band per layer in style order, first layer on top", () => {
    const fsd = style("feature-sliced");
    const laid = styledLayout(
      fsd,
      [
        { id: "s1", layer: "shared" },
        { id: "p1", layer: "pages" },
        { id: "a1", layer: "app" },
        { id: "e1", layer: "entities" },
      ],
      [],
      [],
    );
    const y = (id: string) => laid.centers.get(id)!.y;
    expect(y("a1")).toBeLessThan(y("p1"));
    expect(y("p1")).toBeLessThan(y("e1"));
    expect(y("e1")).toBeLessThan(y("s1"));
    // Empty layers (widgets, features) draw no region.
    expect(laid.regions.map((r) => r.layer)).toEqual(["app", "pages", "entities", "shared"]);
    expect(laid.regions.every((r) => r.shape === "rect")).toBe(true);
  });

  it("columns: stages left to right", () => {
    const laid = styledLayout(
      style("pipeline"),
      [
        { id: "m", layer: "marts" },
        { id: "s", layer: "staging" },
      ],
      [],
      [],
    );
    expect(laid.centers.get("s")!.x).toBeLessThan(laid.centers.get("m")!.x);
  });

  it("rings: core in the centre, shell on a ring around it", () => {
    const laid = styledLayout(
      style("core-shell"),
      [
        { id: "c", layer: "core" },
        { id: "s1", layer: "shell" },
        { id: "s2", layer: "shell" },
      ],
      [],
      [],
    );
    expect(laid.centers.get("c")).toEqual({ x: 0, y: 0 });
    for (const id of ["s1", "s2"]) {
      const p = laid.centers.get(id)!;
      expect(Math.hypot(p.x, p.y)).toBeGreaterThan(CARD_W);
    }
    expect(laid.regions.map((r) => r.layer)).toEqual(["shell", "core"]);
  });

  it("hexagon: domain centre, application ring, presentation left, infrastructure right", () => {
    const laid = styledLayout(
      style("hexagonal"),
      [
        { id: "d", layer: "domain" },
        { id: "a", layer: "application" },
        { id: "p", layer: "presentation" },
        { id: "i", layer: "infrastructure" },
      ],
      ["ghost-caller", "ghost-db"],
      [
        { source: "ghost-caller", target: "p" },
        { source: "i", target: "ghost-db" },
      ],
    );
    const at = (id: string) => laid.centers.get(id)!;
    expect(at("d")).toEqual({ x: 0, y: 0 });
    expect(Math.hypot(at("a").x, at("a").y)).toBeGreaterThan(0);
    expect(at("p").x).toBeLessThan(0);
    expect(at("i").x).toBeGreaterThan(0);
    // Ghosts sit beyond the side they serve.
    expect(at("ghost-caller").x).toBeLessThan(at("p").x);
    expect(at("ghost-db").x).toBeGreaterThan(at("i").x);
    expect(laid.regions.filter((r) => r.shape === "hex").map((r) => r.layer)).toEqual(["domain", "application"]);
  });

  it("orders a band to shorten links to the band above", () => {
    const fsd = style("feature-sliced");
    const laid = styledLayout(
      fsd,
      [
        { id: "p1", layer: "pages" },
        { id: "p2", layer: "pages" },
        { id: "f1", layer: "features" },
        { id: "f2", layer: "features" },
      ],
      [],
      [
        { source: "p1", target: "f2" },
        { source: "p2", target: "f1" },
      ],
    );
    const x = (id: string) => laid.centers.get(id)!.x;
    // p1 is left of p2 by input order; their targets follow.
    expect(x("p1")).toBeLessThan(x("p2"));
    expect(x("f2")).toBeLessThan(x("f1"));
  });

  it("Routes a link that skips a band down a lane clear of the cards in the bands it passes", () => {
    const laid = styledLayout(
      style("feature-sliced"),
      [
        { id: "a", layer: "app" },
        { id: "p", layer: "pages" },
        { id: "w", layer: "widgets" },
        { id: "e", layer: "entities" },
      ],
      [],
      [
        { source: "a", target: "p" },
        { source: "p", target: "w" },
        { source: "w", target: "e" },
        { source: "a", target: "w" },
        { source: "a", target: "e" },
        { source: "p", target: "e" },
      ],
    );
    const at = (id: string) => laid.centers.get(id)!;
    for (const [s, t, passes] of [["a", "w", ["p"]], ["a", "e", ["p", "w"]], ["p", "e", ["w"]]] as const) {
      const route = laid.routes?.get(`${s}\0${t}`);
      expect(route, `${s}→${t}`).toBeDefined();
      for (const id of passes) {
        // Beside the card in that band, never across it.
        const inBand = route!.filter((q) => Math.abs(q.y - at(id).y) <= CARD_H / 2);
        expect(inBand.length).toBeGreaterThan(0);
        for (const q of inBand) expect(Math.abs(q.x - at(id).x)).toBeGreaterThanOrEqual(CARD_W / 2);
      }
    }
    // Two lanes through one band never share a line.
    const xs = ["a\0e", "p\0e"].map((k) => laid.routes!.get(k)!.find((q) => Math.abs(q.y - at("w").y) <= CARD_H / 2)!.x);
    expect(xs[0]).not.toBe(xs[1]);
    // The bands widen to hold their lanes.
    const band = laid.regions.find((r) => r.layer === "widgets")!;
    if (band.shape !== "rect") throw new Error("band");
    for (const x of xs) expect(x > band.x && x < band.x + band.w).toBe(true);
  });

  it("columns: a link that skips a stage runs beside the cards of the stages it passes", () => {
    const pipe = style("pipeline");
    const layers = pipe.layers.map((l) => l.name);
    const laid = styledLayout(
      pipe,
      layers.slice(0, 3).map((layer, i) => ({ id: `s${i}`, layer })),
      [],
      [
        { source: "s0", target: "s1" },
        { source: "s1", target: "s2" },
        { source: "s0", target: "s2" },
      ],
    );
    const mid = laid.centers.get("s1")!;
    const route = laid.routes!.get("s0\0s2")!;
    const inStage = route.filter((q) => Math.abs(q.x - mid.x) <= CARD_W / 2);
    expect(inStage.length).toBeGreaterThan(0);
    for (const q of inStage) expect(Math.abs(q.y - mid.y)).toBeGreaterThanOrEqual(CARD_H / 2);
  });
});

describe("classifyStyledEdge", () => {
  const hex = style("hexagonal");
  const fsd = style("feature-sliced");
  it("a legal cross-layer dependency is implied by the drawing", () => {
    expect(classifyStyledEdge(hex, "application", "domain")).toBe("implied");
    expect(classifyStyledEdge(hex, "infrastructure", "application")).toBe("implied");
    expect(classifyStyledEdge(hex, "presentation", "application")).toBe("implied");
    expect(classifyStyledEdge(fsd, "pages", "entities")).toBe("implied");
  });
  it("a forbidden pair is a violation; same-layer is implied unless the style isolates", () => {
    expect(classifyStyledEdge(hex, "domain", "application")).toBe("violation");
    expect(classifyStyledEdge(hex, "presentation", "infrastructure")).toBe("violation");
    expect(classifyStyledEdge(fsd, "entities", "pages")).toBe("violation");
    expect(classifyStyledEdge(hex, "application", "application")).toBe("implied");
    expect(classifyStyledEdge(fsd, "pages", "pages")).toBe("violation");
    expect(classifyStyledEdge(hex, undefined, "domain")).toBe("plain");
  });
  it("traffic across the container edge enters inbound and leaves outbound", () => {
    expect(classifyStyledEdge(hex, undefined, "presentation", { sourceGhost: true })).toBe("implied");
    expect(classifyStyledEdge(hex, undefined, "domain", { sourceGhost: true })).toBe("violation");
    expect(classifyStyledEdge(hex, "infrastructure", undefined, { targetGhost: true })).toBe("implied");
    expect(classifyStyledEdge(hex, "domain", undefined, { targetGhost: true })).toBe("violation");
    expect(classifyStyledEdge(fsd, "shared", undefined, { targetGhost: true })).toBe("implied");
  });
});

describe("composition", () => {
  const hex = style("hexagonal");
  it("hexagon: composition is a band above the whole drawing", () => {
    const laid = styledLayout(
      hex,
      [
        { id: "w", layer: "composition" },
        { id: "d", layer: "domain" },
        { id: "a", layer: "application" },
        { id: "p", layer: "presentation" },
        { id: "i", layer: "infrastructure" },
      ],
      [],
      [
        { source: "w", target: "p" },
        { source: "w", target: "i" },
      ],
    );
    const at = (id: string) => laid.centers.get(id)!;
    expect(at("d")).toEqual({ x: 0, y: 0 });
    for (const id of ["d", "a", "p", "i"]) expect(at("w").y).toBeLessThan(at(id).y);
    expect(laid.regions.find((r) => r.layer === "composition")?.shape).toBe("rect");
    expect(laid.regions.filter((r) => r.shape === "hex").map((r) => r.layer)).toEqual(["domain", "application"]);
  });
  it("an import of a library is judged by the importer's own matrix", () => {
    const lib = style("library");
    // application may depend on domain — here the library's domain.
    expect(
      classifyStyledEdge(hex, "application", undefined, {
        targetGhost: true,
        otherDef: lib,
        otherLayer: "domain",
      }),
    ).toBe("implied");
    // presentation may not depend on domain, in any container.
    expect(
      classifyStyledEdge(hex, "presentation", undefined, {
        targetGhost: true,
        otherDef: lib,
        otherLayer: "domain",
      }),
    ).toBe("violation");
    // The library's facade: reached from the driven side, not by the matrix.
    expect(
      classifyStyledEdge(hex, "infrastructure", undefined, {
        targetGhost: true,
        otherDef: lib,
        otherLayer: "composition",
      }),
    ).toBe("implied");
    expect(
      classifyStyledEdge(hex, "application", undefined, {
        targetGhost: true,
        otherDef: lib,
        otherLayer: "composition",
      }),
    ).toBe("violation");
    // Without a module-imported target the outbound rule still applies.
    expect(
      classifyStyledEdge(hex, "application", undefined, {
        targetGhost: true,
        otherDef: hex,
        otherLayer: "presentation",
      }),
    ).toBe("violation");
  });

  it("composition may wire every layer, and no layer may import it", () => {
    expect(classifyStyledEdge(hex, "composition", "infrastructure")).toBe("implied");
    expect(classifyStyledEdge(hex, "composition", "presentation")).toBe("implied");
    expect(classifyStyledEdge(hex, "presentation", "composition")).toBe("violation");
    expect(classifyStyledEdge(hex, "domain", "composition")).toBe("violation");
    expect(classifyStyledEdge(style("library"), undefined, "composition", { sourceGhost: true })).toBe("implied");
  });
});

describe("styled scene", () => {
  it("Draws the links the drawing implies quiet, routed clear of the cards like the rest", async () => {
    const model = {
      version: 1,
      nodes: [
        { id: "sys", kind: "system", name: "Sys" },
        { id: "ui", kind: "container", name: "UI", parentId: "sys", style: "feature-sliced" },
        { id: "a", kind: "component", name: "app", parentId: "ui", layer: "app" },
        { id: "p", kind: "component", name: "pages", parentId: "ui", layer: "pages" },
        { id: "e", kind: "component", name: "entities", parentId: "ui", layer: "entities" },
      ],
      links: [
        { id: "l1", src: "a", dst: "p", label: "", kind: "depends" },
        { id: "l2", src: "p", dst: "e", label: "", kind: "depends" },
        { id: "l3", src: "a", dst: "e", label: "", kind: "depends" },
      ],
      groups: [],
    } as unknown as ScryModel;
    const scene = await buildDiagramScene(model, "ui", null);
    expect(scene.mode).toBe("styled");
    const skip = scene.edges.find((e) => e.source === "a" && e.target === "e")!;
    // Legal, so quiet — and routed all the same.
    expect(skip.implied).toBe(true);
    const pages = scene.nodes.find((n) => n.id === "p")!;
    const inBand = (skip.waypoints ?? []).filter((q) => q.y >= pages.y && q.y <= pages.y + CARD_H);
    expect(inBand.length).toBeGreaterThan(0);
    for (const q of inBand) expect(q.x < pages.x || q.x > pages.x + CARD_W).toBe(true);
  });

  it("Draws a link that leaves the container plain: it says what the drawing cannot", async () => {
    const model = {
      version: 1,
      nodes: [
        { id: "sys", kind: "system", name: "Sys" },
        { id: "ui", kind: "container", name: "UI", parentId: "sys", style: "feature-sliced" },
        { id: "a", kind: "component", name: "app", parentId: "ui", layer: "app" },
        { id: "e", kind: "component", name: "entities", parentId: "ui", layer: "entities" },
        { id: "cli", kind: "container", name: "CLI", parentId: "sys" },
      ],
      links: [
        { id: "l1", src: "a", dst: "e", label: "", kind: "depends" },
      ],
      groups: [],
    } as unknown as ScryModel;
    // The CLI imports app: a ghost, wired to app.
    const report = {
      health: { disconnected: [] },
      anchors: [],
      derived: { resolvedEdges: [{ srcNode: "cli", dstNode: "a", count: 2 }] },
      styles: [],
      structural: { violations: [] },
    } as unknown as ModelHealthReport;
    const scene = await buildDiagramScene(model, "ui", report);
    const inbound = scene.edges.find((e) => e.target === "a")!;
    expect(scene.nodes.find((n) => n.id === inbound.source)?.reference).toBe(true);
    expect(inbound.implied).toBe(false);
    expect(scene.edges.find((e) => e.source === "a" && e.target === "e")?.implied).toBe(true);
  });
});
