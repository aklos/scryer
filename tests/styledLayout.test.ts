import { describe, expect, it } from "vitest";
import { builtinStyles } from "../src/styles";
import { isImpliedEdge, styledLayout, CARD_W } from "../src/layout/styled";

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
});

describe("isImpliedEdge", () => {
  const hex = style("hexagonal");
  const fsd = style("feature-sliced");
  it("hides steps into the innermost layer and adapters onto ports", () => {
    expect(isImpliedEdge(hex, "application", "domain", "depends")).toBe(true);
    expect(isImpliedEdge(hex, "infrastructure", "application", "implements")).toBe(true);
    expect(isImpliedEdge(fsd, "pages", "widgets", "calls")).toBe(true); // adjacent step on rows
  });
  it("keeps same-layer links, illegal pairs and long jumps visible", () => {
    expect(isImpliedEdge(hex, "application", "application", "uses")).toBe(false);
    expect(isImpliedEdge(hex, "domain", "application", "calls")).toBe(false);
    expect(isImpliedEdge(hex, "presentation", "application", "calls")).toBe(false);
    expect(isImpliedEdge(fsd, "pages", "entities", "calls")).toBe(false);
  });
});
