/**
 * The map's layout held to a score: every box-tier level of this repo's own
 * model (`fixtures/scryer-model.json`, symbols folded into their components)
 * is built as the app builds it, its edges run as the map draws them, and the
 * faults a reader trips on are counted (`layoutQuality`).
 *
 * Overlapping cards fail outright. The other counts are a ratchet against
 * `fixtures/layout-baseline.json`: a level may get better, never worse. After
 * a layout change that improves the map, record the new counts with
 * `UPDATE_LAYOUT_BASELINE=1`; `LAYOUT_REPORT=1` prints them per level.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { buildDiagramScene, CARD_H, CARD_W } from "../src/widgets/diagram-canvas/diagramLayout";
import { edgeLines, linePoints } from "../src/widgets/diagram-canvas/edgeLines";
import { layoutQuality, type ScoredBox } from "../src/widgets/diagram-canvas/layoutQuality";
import type { ScryModel } from "../src/entities/model/viewmodel";
import type { ModelHealthReport } from "../src/entities/model/health";

const box = (id: string, x: number, y: number): ScoredBox => ({ id, x, y, w: 100, h: 80 });
const line = (id: string, source: string, target: string, ...points: [number, number][]) => ({
  id,
  source,
  target,
  points: points.map(([x, y]) => ({ x, y })),
});

describe("layoutQuality", () => {
  it("Counts two cards that overlap, and none for cards apart", () => {
    const q = layoutQuality([box("a", 0, 0), box("b", 50, 40), box("c", 300, 0)], []);
    expect(q.overlaps).toEqual([["a", "b"]]);
  });

  it("Counts an edge that runs through a card other than its own two, but not one that grazes its border", () => {
    const boxes = [box("a", 0, 0), box("m", 200, 0), box("b", 400, 0)];
    const through = line("e", "a", "b", [100, 40], [400, 40]);
    const grazing = line("f", "a", "b", [100, 0], [400, 0]);
    expect(layoutQuality(boxes, [through]).throughCards).toEqual([{ edge: "e", card: "m" }]);
    expect(layoutQuality(boxes, [grazing]).throughCards).toEqual([]);
  });

  it("Counts each point where two edges cross, but not where they meet at a card", () => {
    const x = [line("e", "a", "b", [0, 0], [100, 100]), line("f", "c", "d", [0, 100], [100, 0])];
    expect(layoutQuality([], x).crossings).toBe(1);
    const meet = [line("e", "a", "b", [0, 0], [100, 100]), line("f", "a", "c", [0, 0], [100, 0])];
    expect(layoutQuality([], meet).crossings).toBe(0);
  });

  it("Counts a stretch where two edges run along one line, but not two that only touch", () => {
    const along = [line("e", "a", "b", [0, 0], [100, 0]), line("f", "c", "d", [50, 0], [200, 0])];
    expect(layoutQuality([], along).sharedRuns).toBe(1);
    const touching = [line("e", "a", "b", [0, 0], [100, 0]), line("f", "c", "d", [100, 0], [200, 0])];
    expect(layoutQuality([], touching).sharedRuns).toBe(0);
  });

  it("Counts each turn along an edge, not a straight point it passes", () => {
    const q = layoutQuality([], [line("e", "a", "b", [0, 0], [0, 50], [0, 100], [100, 100], [100, 200])]);
    expect(q.bends).toBe(2);
  });
});

const fixture = JSON.parse(readFileSync(new URL("./fixtures/scryer-model.json", import.meta.url), "utf8")) as {
  model: ScryModel;
  report: ModelHealthReport;
};
const baselineUrl = new URL("./fixtures/layout-baseline.json", import.meta.url);

type Score = { crossings: number; throughCards: number; sharedRuns: number; bends: number };
type Level = Score & { overlaps: [string, string][] };

/** Every box-tier level: the root, and each node whose children are boxes. */
async function scoreLevels(): Promise<Map<string, Level>> {
  const { model, report } = fixture;
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const name = (id: string | null): string => {
    if (id === null) return "(root)";
    const n = byId.get(id)!;
    return n.parentId ? `${name(n.parentId)} / ${n.name}` : n.name;
  };
  const focuses = [null, ...model.nodes.filter((n) => n.kind !== "symbol").map((n) => n.id)];
  const out = new Map<string, Level>();
  for (const focus of focuses) {
    const scene = await buildDiagramScene(model, focus, report);
    if (scene.mode === "code" || scene.nodes.length === 0) continue;
    const boxes = scene.nodes.map((n) => ({ id: n.id, x: n.x, y: n.y, w: CARD_W, h: CARD_H }));
    const boxOf = new Map(boxes.map((b) => [b.id, b]));
    const lines = edgeLines(scene, boxes);
    const q = layoutQuality(
      boxes,
      scene.edges
        .filter((e) => boxOf.has(e.source) && boxOf.has(e.target))
        .map((e) => ({ id: e.id, source: e.source, target: e.target, points: linePoints(e, lines.get(e.id)!, boxOf) })),
    );
    out.set(name(focus), {
      overlaps: q.overlaps,
      crossings: q.crossings,
      throughCards: q.throughCards.length,
      sharedRuns: q.sharedRuns,
      bends: q.bends,
    });
  }
  return out;
}

describe("this repo's map", async () => {
  const levels = await scoreLevels();
  if (process.env.LAYOUT_REPORT)
    console.table(Object.fromEntries([...levels].map(([k, v]) => [k, { ...v, overlaps: v.overlaps.length }])));
  if (process.env.UPDATE_LAYOUT_BASELINE) {
    const record = Object.fromEntries(
      [...levels].map(([k, { crossings, throughCards, sharedRuns, bends }]) => [k, { crossings, throughCards, sharedRuns, bends }]),
    );
    writeFileSync(baselineUrl, JSON.stringify(record, null, 2) + "\n");
  }
  const baseline = JSON.parse(readFileSync(baselineUrl, "utf8")) as Record<string, Score>;

  it("Never overlaps two cards on any level", () => {
    const overlapping = [...levels].filter(([, s]) => s.overlaps.length).map(([k, s]) => [k, s.overlaps]);
    expect(overlapping).toEqual([]);
  });

  it("Draws no level with more crossings, edges through cards, shared runs or bends than its recorded baseline", () => {
    const worse = [...levels].flatMap(([k, s]) => {
      const b = baseline[k];
      if (!b) return [`${k}: no baseline`];
      return (["crossings", "throughCards", "sharedRuns", "bends"] as const)
        .filter((m) => s[m] > b[m])
        .map((m) => `${k}: ${m} ${b[m]} → ${s[m]}`);
    });
    expect(worse).toEqual([]);
  });

  it("Draws no edge through a card that is not one of its ends, on any level", () => {
    expect([...levels].filter(([, s]) => s.throughCards).map(([k, s]) => [k, s.throughCards])).toEqual([]);
  });
});
