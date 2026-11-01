/** Claims for the top bar's changes toggle. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ChangesToggle, changesLabel } from "../src/widgets/top-bar/TopBar";
import { NO_CHANGES } from "../src/entities/model/changes";

const base = (o: Partial<{ against: string; ahead: number; dirty: boolean }>) => ({ commit: "c", ahead: 0, dirty: false, ...o });

describe("TopBar", () => {
  it("Says what the checkout changed: the commits it is ahead of the branch it left, and whether anything is uncommitted", () => {
    expect(changesLabel(base({ dirty: true }))).toBe("Uncommitted changes");
    expect(changesLabel(base({ against: "main", ahead: 3 }))).toBe("3 commits ahead of main");
    expect(changesLabel(base({ against: "main", ahead: 1, dirty: true }))).toBe("1 commit ahead of main · uncommitted");
    expect(changesLabel(base({}))).toBeNull();
    expect(changesLabel(null)).toBeNull();
  });

  it("While the checkout changed nothing, draws no changes toggle; otherwise a pressed toggle while the changes are shown", () => {
    const draw = (b: ReturnType<typeof base> | null, on: boolean) =>
      renderToStaticMarkup(<ChangesToggle base={b} changes={NO_CHANGES} on={on} onToggle={() => {}} />);
    expect(draw(base({}), false)).toBe("");
    expect(draw(base({ dirty: true }), true)).toContain('aria-pressed="true"');
    expect(draw(base({ dirty: true }), false)).toContain("nothing in the model changed");
  });
});
