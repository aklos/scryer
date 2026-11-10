import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ReactFlowProvider } from "@xyflow/react";
import { DotNode } from "../src/widgets/diagram-canvas/DiagramView";
import { NodeHandles } from "../src/shared/diagram/NodeHandles";
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
  it("Draws a symbol as its type's glyph from the tree (code, data shape, visual) at its given size, with no class line under its name", () => {
    const code = draw(sym({}));
    expect(code).toContain("lucide-square-function");
    expect(code).toMatch(/<svg[^>]*width="40"[^>]*height="40"/);
    expect(draw(sym({ symbolClass: "model" }))).toContain("lucide-braces");
    const visual = draw(sym({ symbolClass: "visual", dotSize: 18 }));
    expect(visual).toContain("lucide-eye");
    expect(visual).toMatch(/<svg[^>]*width="18"/);
    expect(code).not.toContain("rounded-full");
    // The name is the only text: the class is said by the glyph, and named
    // only in its label.
    expect(code.replace(/<[^>]+>/g, "")).toBe("useModelHealth");
    expect(code).toContain('aria-label="useModelHealth, code"');
  });

  it("Draws a symbol with claims in full tone, one with none in a muted tone, and one with a failing claim in red unless it lives elsewhere; a todo claim adds no hue", () => {
    expect(draw(sym({}))).toContain("text-[var(--text)] ");
    expect(draw(sym({ hasClaims: false }))).toMatch(/justify-center rounded-md text-\[var\(--text-muted\)\]/);
    expect(draw(sym({}), "failing")).toContain(CLAIM_TONE.failing);
    expect(draw(sym({}), "planned")).not.toContain(CLAIM_TONE.planned);
    // A symbol from elsewhere tells its state on its own level.
    expect(draw(sym({ reference: true }), "failing")).not.toContain(CLAIM_TONE.failing);
  });

  it("Draws a referenced caller or dependency as its own kind's glyph", () => {
    const ref = draw(sym({ kind: "component", reference: true }));
    expect(ref).toContain("lucide-component");
    expect(ref).not.toContain("lucide-square-function");
  });

  it("Rests every symbol at full opacity, leaves and references alike, fading only what a selection misses", () => {
    expect(draw(sym({ hasClaims: false, degree: 0 }))).not.toMatch(/opacity-\d/);
    expect(draw(sym({ kind: "component", reference: true }))).not.toMatch(/opacity-\d/);
  });

  it("Offers no connection anywhere on a card or symbol, so the pointer reads the same across it", () => {
    const handles = renderToStaticMarkup(
      <ReactFlowProvider>
        <NodeHandles />
      </ReactFlowProvider>,
    );
    for (const html of [draw(sym({})), handles]) {
      expect(html).toContain("react-flow__handle");
      expect(html).not.toContain("connectionindicator");
      expect(html).not.toMatch(/class="[^"]*react-flow__handle[^"]*\bconnectable\b/);
    }
  });
});
