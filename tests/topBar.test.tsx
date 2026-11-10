/** Claims for the top bar's changes status. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ChangesStatus, changesLabel } from "../src/widgets/top-bar/TopBar";
import { NO_CHANGES } from "../src/entities/model/changes";
import { stageForWidth } from "../src/widgets/top-bar/useShedStage";

const base = (o: Partial<{ against: string; ahead: number; dirty: boolean }>) => ({ commit: "c", ahead: 0, dirty: false, ...o });

describe("TopBar", () => {
  it("Says what the checkout changed: the commits it is ahead of the branch it left, and whether anything is uncommitted", () => {
    expect(changesLabel(base({ dirty: true }))).toBe("Uncommitted changes");
    expect(changesLabel(base({ against: "main", ahead: 3 }))).toBe("3 commits ahead of main");
    expect(changesLabel(base({ against: "main", ahead: 1, dirty: true }))).toBe("1 commit ahead of main · uncommitted");
    expect(changesLabel(base({}))).toBeNull();
    expect(changesLabel(null)).toBeNull();
  });

  it("While the checkout changed nothing, draws no changes status; otherwise a status that opens Activity, never a toggle", () => {
    const draw = (b: ReturnType<typeof base> | null) =>
      renderToStaticMarkup(<ChangesStatus base={b} changes={NO_CHANGES} onOpen={() => {}} />);
    expect(draw(base({}))).toBe("");
    const html = draw(base({ dirty: true }));
    expect(html).toContain("nothing in the model changed");
    expect(html).toContain("Open Activity");
    expect(html).not.toContain("aria-pressed");
  });

  it("When the window widens, takes back words only down to the stage whose measured width now fits", () => {
    // Stage 0 needed 1200px, stage 1 needed 1000px, stage 2 needed 900px.
    const need = [1200, 1000, 900];
    expect(stageForWidth(need, 3, 899)).toBe(3);
    expect(stageForWidth(need, 3, 900)).toBe(2);
    expect(stageForWidth(need, 3, 1100)).toBe(1);
    expect(stageForWidth(need, 3, 1200)).toBe(0);
    // A stage never measured is not returned to blind.
    expect(stageForWidth([undefined, 1000], 2, 5000)).toBe(1);
  });
});
