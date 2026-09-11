import { describe, expect, it } from "vitest";
import type { ModelHealthReport, StyleViolation } from "../src/health";
import { structuralBySubtree, structuralNotice } from "../src/health";
import { buildReviewIndex } from "../src/special/NeedsReviewPage";
import { buildDiagramScene } from "../src/diagramLayout";
import type { ScryModel } from "../src/viewmodel";

/** A hexagonal service with a declared domain → infrastructure link (which the
 *  matrix forbids) and a legal application → domain one, beside an unstyled
 *  container. */
const model = {
  version: 1,
  nodes: [
    { id: "sys", kind: "system", name: "Sys" },
    { id: "svc", kind: "container", name: "Svc", parentId: "sys", style: "hexagonal" },
    { id: "app", kind: "component", name: "Checkout", parentId: "svc", layer: "application" },
    { id: "dom", kind: "component", name: "Orders", parentId: "svc", layer: "domain" },
    { id: "infra", kind: "component", name: "Postgres", parentId: "svc", layer: "infrastructure" },
    { id: "bare", kind: "container", name: "Bare", parentId: "sys" },
    { id: "k", kind: "component", name: "K", parentId: "bare" },
  ],
  links: [
    { id: "l-bad", src: "dom", dst: "infra", label: "", kind: "depends" },
    { id: "l-ok", src: "app", dst: "dom", label: "", kind: "depends" },
  ],
  groups: [],
} as unknown as ScryModel;

const forbidden: StyleViolation = {
  kind: "forbidden_link",
  node: "dom",
  other: "infra",
  file: "svc/domain/order.rs",
  container: "svc",
  detail: "declared link l-bad: 'Orders' (domain) → 'Postgres' (infrastructure) is illegal in style 'hexagonal'",
};
const unstyled: StyleViolation = {
  kind: "unstyled",
  node: "bare",
  file: "bare/",
  container: "bare",
  detail: "'Bare' declares no architectural style",
};

const reportWith = (violations: StyleViolation[]) =>
  ({
    health: { disconnected: [] },
    anchors: [],
    derived: { resolvedEdges: [] },
    styles: [],
    structural: { violations },
  }) as unknown as ModelHealthReport;

const ASK = "ask the agent to choose the architecture this code should have and refactor to it";

describe("structural notice", () => {
  it("charges a violation to its node and every ancestor", () => {
    const by = structuralBySubtree(model, reportWith([forbidden, unstyled]));
    expect(by.get("dom")).toEqual([forbidden]);
    expect(by.get("svc")).toEqual([forbidden]);
    expect(by.get("bare")).toEqual([unstyled]);
    expect(by.get("sys")).toHaveLength(2);
    expect(by.has("infra")).toBe(false);
  });

  it("tells the user to have the agent choose an architecture and refactor", () => {
    expect(structuralNotice([])).toBeNull();
    expect(structuralNotice([unstyled])).toEqual({
      unstyledOnly: true,
      text: `No architectural style declared — ${ASK}`,
    });
    expect(structuralNotice([forbidden, unstyled])).toEqual({
      unstyledOnly: false,
      text: `Structurally invalid (2 violations) — ${ASK}`,
    });
  });
});

describe("needs review: structural violations", () => {
  const index = (violations: StyleViolation[]) =>
    buildReviewIndex(model, reportWith(violations), [], new Set(), new Set());

  it("groups violations by container and counts all but unstyled containers", () => {
    const base = index([]).total;
    const idx = index([forbidden, unstyled]);
    expect(idx.structural.map((g) => [g.containerId, g.name, g.violations.length])).toEqual([
      ["svc", "Svc", 1],
      ["bare", "Bare", 1],
    ]);
    expect(idx.total).toBe(base + 1);
    expect(index([unstyled]).total).toBe(base);
  });
});

describe("styled map: red comes from the report", () => {
  const edge = async (violations: StyleViolation[], id: string) =>
    (await buildDiagramScene(model, "svc", reportWith(violations))).edges.find((e) => e.id === id);

  it("marks red only the edges the structural report charges", async () => {
    const quiet = await edge([], "l-bad");
    expect(quiet?.violation).toBeUndefined();
    const red = await edge([forbidden], "l-bad");
    expect(red?.violation).toBe(forbidden.detail);
    expect(red?.implied).toBe(false);
    expect((await edge([forbidden], "l-ok"))?.implied).toBe(true);
  });
});
