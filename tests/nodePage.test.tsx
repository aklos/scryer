/** Claims for `NodePage`. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ShowChangesToggle } from "../src/pages/node/NodePage";

describe("NodePage", () => {
  it.todo("If the selected node or group no longer exists, then says the page no longer exists");

  it("While the page's changes are shown in full, draws its Show changes toggle pressed", () => {
    const draw = (on: boolean) => renderToStaticMarkup(<ShowChangesToggle on={on} onToggle={() => {}} />);
    expect(draw(true)).toContain('aria-pressed="true"');
    expect(draw(false)).toContain('aria-pressed="false"');
    expect(draw(false)).toContain("Show changes");
  });
});
