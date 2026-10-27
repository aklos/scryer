import { describe, expect, it } from "vitest";
import type { ModelHealthReport, StyleViolation } from "../src/entities/model/health";
import { noStyleNotice, structuralBySubtree, structuralNotice, unstyledContainers } from "../src/entities/model/health";
import { buildDiagramScene } from "../src/widgets/diagram-canvas/diagramLayout";
import type { ScryModel } from "../src/entities/model/viewmodel";

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

describe("structural notice", () => {
  it("Charges a violation to its node and every ancestor", () => {
    const by = structuralBySubtree(model, reportWith([forbidden, unstyled]));
    expect(by.get("dom")).toEqual([forbidden]);
    expect(by.get("svc")).toEqual([forbidden]);
    expect(by.get("sys")).toEqual([forbidden]);
    expect(by.has("infra")).toBe(false);
  });

  it("Never counts a container with no style chosen as a violation", () => {
    const report = reportWith([unstyled]);
    expect(structuralBySubtree(model, report).size).toBe(0);
    expect([...unstyledContainers(report)]).toEqual(["bare"]);
  });

  it("When a container has no style chosen, notes it on the container and names it on the system", () => {
    const none = new Set(["bare"]);
    expect(noStyleNotice(none, { id: "bare", kind: "container" }, model)?.text).toMatch(/^No architectural style chosen yet/);
    expect(noStyleNotice(none, { id: "sys", kind: "system" }, model)?.containers).toEqual(["bare"]);
    expect(noStyleNotice(none, { id: "svc", kind: "container" }, model)).toBeNull();
  });

  it("When a container's code breaks its style, names the style on the container and on the component", () => {
    expect(structuralNotice([], { id: "svc", kind: "container" }, model)).toBeNull();
    expect(structuralNotice([forbidden], { id: "svc", kind: "container" }, model)).toBe(
      "1 violation of this container's hexagonal style — ask the agent to refactor the code to it",
    );
    expect(structuralNotice([forbidden], { id: "dom", kind: "component" }, model)).toBe(
      "1 violation of Svc's hexagonal style here — ask the agent to refactor the code to it",
    );
  });

  it("When the violations lie in containers below, names those containers instead of asking for a style", () => {
    expect(structuralNotice([forbidden], { id: "sys", kind: "system" }, model)).toBe(
      "Inside: Svc breaks its style (1 violation)",
    );
  });
});

describe("styled map: red comes from the report", () => {
  const edge = async (violations: StyleViolation[], id: string) =>
    (await buildDiagramScene(model, "svc", reportWith(violations))).edges.find((e) => e.id === id);

  it("marks red only the edges the structural report charges", async () => {
    const quiet = await edge([], "l-bad");
    expect(quiet?.violations).toBeUndefined();
    const red = await edge([forbidden], "l-bad");
    expect(red?.violations).toEqual([forbidden.detail]);
    expect(red?.conforming).toBe(false);
    expect((await edge([forbidden], "l-ok"))?.conforming).toBe(true);
  });

  it("carries every reason charged on one pair, each once", async () => {
    const second = { ...forbidden, detail: `${forbidden.detail} (second import)` };
    const red = await edge([forbidden, second, forbidden], "l-bad");
    expect(red?.violations).toEqual([forbidden.detail, second.detail]);
  });
});
