import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { StyleViolation } from "../src/entities/model/health";
import { StructuralCount } from "../src/entities/model/StructuralCount";
import { StructuralViolationList } from "../src/pages/node/NodePage";

const model = {
  nodes: [
    { id: "sys", kind: "system", name: "Sys" },
    { id: "svc", kind: "container", name: "Svc", parentId: "sys" },
    { id: "dom", kind: "component", name: "Orders", parentId: "svc" },
    { id: "infra", kind: "component", name: "Postgres", parentId: "svc" },
    { id: "bare", kind: "container", name: "Bare", parentId: "sys" },
  ],
} as never;

const forbidden: StyleViolation = {
  kind: "layer_violation",
  node: "dom",
  other: "infra",
  file: "svc/domain/order.rs",
  container: "svc",
  detail: "domain imports infrastructure: order.rs → pg.rs",
};
const misplaced: StyleViolation = {
  kind: "misplaced",
  node: "infra",
  file: "svc/domain/pg.rs",
  container: "svc",
  detail: "pg.rs sits in domain's directory but belongs to infrastructure",
};
const unstyled: StyleViolation = {
  kind: "unstyled",
  node: "bare",
  file: "bare/",
  container: "bare",
  detail: "'Bare' declares no architectural style",
};

describe("locating structural violations", () => {
  it("lists each violation under its container with component, file and detail", () => {
    const html = renderToStaticMarkup(
      <StructuralViolationList
        violations={[forbidden, misplaced, unstyled]}
        model={model}
        onSelectNode={() => {}}
      />,
    );
    // Grouped: Svc's two violations come before Bare's heading.
    const svc = html.indexOf(">Svc<"), bare = html.indexOf(">Bare<");
    expect(svc).toBeGreaterThan(-1);
    expect(bare).toBeGreaterThan(svc);
    expect(html.indexOf(misplaced.file)).toBeLessThan(bare);
    expect(html).toContain("2 violations");
    // Each names the charged component (and the one it reaches), the file, and what is wrong.
    expect(html).toMatch(/>Orders<.*→.*>Postgres</);
    for (const v of [forbidden, misplaced, unstyled]) {
      expect(html).toContain(v.file);
      expect(html).toContain(v.detail.replace(/'/g, "&#x27;").replace(/>/g, "&gt;"));
    }
    expect(html).toContain("misplaced file");
  });

  it("counts a subtree's violations, orange when only styles are undeclared", () => {
    expect(renderToStaticMarkup(<StructuralCount violations={[]} />)).toBe("");
    const red = renderToStaticMarkup(<StructuralCount violations={[forbidden, unstyled]} />);
    expect(red).toContain('data-structural-count="2"');
    expect(red).toContain("text-red-600");
    expect(red).toContain("2 structural violations in this subtree");
    const orange = renderToStaticMarkup(<StructuralCount violations={[unstyled]} />);
    expect(orange).toContain("text-orange-600");
    expect(orange).toContain("declares no architectural style");
  });
});
