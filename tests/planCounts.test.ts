/**
 * The two pending numbers (`src/changeMarks.ts`) — the canvas side of what
 * `pending_elements` / `plan_carrier_count` count in Rust.
 *
 * The bug these pin: the app reported only CARRIERS while the agent's header
 * and `get_pending` reported ELEMENTS, so the same plan read "1 pending" in the
 * status bar and "3 pending" in the terminal. Both numbers now travel together,
 * and both sides must exclude vagrant (drift) content identically.
 */
import { describe, expect, it } from "vitest";
import { planCountLabel, planCounts } from "../src/changeMarks";
import { planDiff } from "../src/planDiff";
import type { ScryModel } from "../src/viewmodel";

const model = (nodes: ScryModel["nodes"]): ScryModel => ({
  version: "0.3",
  nodes,
  links: [],
  groups: [],
});

const node = (id: string, extra: Partial<ScryModel["nodes"][number]> = {}) =>
  ({ id, kind: "component", name: id.toUpperCase(), ...extra }) as ScryModel["nodes"][number];

describe("planCounts", () => {
  it("counts every diverging element, and the carriers they fold under", () => {
    const committed = model([
      node("a", { responsibilities: [{ id: "r1", statement: "does one thing" }] }),
      node("b"),
    ]);
    const planned = model([
      node("a", {
        responsibilities: [
          { id: "r1", statement: "does one thing" },
          { id: "r2", statement: "does a second thing" },
          { id: "r3", statement: "does a third thing" },
        ],
      }),
      node("b"),
      node("c"),
    ]);

    // Three elements owed (r2, r3, node c) across two carriers (a, c).
    expect(planCounts(planDiff(committed, planned), planned, committed)).toEqual({
      elements: 3,
      carriers: 2,
    });
  });

  it("leaves vagrant content out — a drift verdict is not implement-queue work", () => {
    const committed = model([node("a")]);
    const planned = model([
      node("a", {
        responsibilities: [{ id: "rv", statement: "code already does this", vagrant: true }],
      }),
    ]);

    expect(planCounts(planDiff(committed, planned), planned, committed)).toEqual({
      elements: 0,
      carriers: 0,
    });
  });

  it("phrases both numbers as one line, singular carrier included", () => {
    expect(planCountLabel({ elements: 23, carriers: 8 })).toBe("23 across 8 nodes");
    expect(planCountLabel({ elements: 3, carriers: 1 })).toBe("3 across 1 node");
  });
});
