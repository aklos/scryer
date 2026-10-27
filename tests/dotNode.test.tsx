import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ReactFlowProvider } from "@xyflow/react";
import { DotNode } from "../src/widgets/diagram-canvas/DiagramView";
import { CLAIM_TONE } from "../src/entities/model/claimState";
import type { DiagramNode } from "../src/widgets/diagram-canvas/diagramLayout";

const sym = (over: Partial<DiagramNode>) =>
  ({ id: "s", kind: "symbol", name: "useModelHealth", symbolClass: "code", dotSize: 40, hasClaims: true, degree: 3, reference: false, ...over }) as DiagramNode;

const draw = (node: DiagramNode, attention: "failing" | "planned" | null = null) =>
  renderToStaticMarkup(
    <ReactFlowProvider>
      <DotNode {...({ id: node.id, data: { node, selected: false, attention } } as unknown as Parameters<typeof DotNode>[0])} />
    </ReactFlowProvider>,
  );

describe("code-tier symbol", () => {
  it("Draws a symbol as its type's glyph from the tree (code, data shape, visual) sized by fan-in, with no class line under its name", () => {
    const code = draw(sym({}));
    expect(code).toContain("lucide-code");
    expect(code).toMatch(/<svg[^>]*width="40"[^>]*height="40"/);
    expect(draw(sym({ symbolClass: "model" }))).toContain("lucide-database");
    const visual = draw(sym({ symbolClass: "visual", dotSize: 18 }));
    expect(visual).toContain("lucide-eye");
    expect(visual).toMatch(/<svg[^>]*width="18"/);
    expect(code).not.toContain("rounded-full");
    // The name is the only text: the class is said by the glyph, and named
    // only in its label.
    expect(code.replace(/<[^>]+>/g, "")).toBe("useModelHealth");
    expect(code).toContain('aria-label="useModelHealth, code"');
  });

  it("Draws a symbol with claims in full tone, one with none in a ghost tone, and one with a failing claim in red unless it lives elsewhere; a todo claim adds no hue", () => {
    expect(draw(sym({}))).toContain("text-[var(--text-secondary)] ");
    expect(draw(sym({ hasClaims: false }))).toMatch(/justify-center rounded-md text-\[var\(--text-ghost\)\]/);
    expect(draw(sym({}), "failing")).toContain(CLAIM_TONE.failing);
    expect(draw(sym({}), "planned")).not.toContain(CLAIM_TONE.planned);
    // A symbol from elsewhere tells its state on its own level.
    expect(draw(sym({ reference: true }), "failing")).not.toContain(CLAIM_TONE.failing);
  });
});
