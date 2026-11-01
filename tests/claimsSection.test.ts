/** Todo claims for `ClaimsSection`. */
import { describe, expect, it } from "vitest";
import { buildRespDiff, orderByConcern } from "../src/widgets/node-page-sections/ClaimsSection";

describe("ClaimsSection", () => {
  it.todo("Lists a node's claims in concern groups ordered A to Z, with the untagged core flow after them in its own order");
  it("While the changes are hidden, marks claims added or reworded since the base and leaves out the removed; shown, lists the removed too", () => {
    const base = [
      { id: "kept", statement: "Saves a note" },
      { id: "reworded", statement: "Tags a note" },
      { id: "removed", statement: "Prints a note" },
    ];
    const now = [
      { id: "kept", statement: "Saves a note" },
      { id: "reworded", statement: "Tags a draft" },
      { id: "added", statement: "Shares a note" },
    ];
    const kinds = (show: boolean) => buildRespDiff(now, base, show).map((r) => `${r.resp.id}:${r.kind}`);
    expect(kinds(false)).toEqual(["kept:unchanged", "reworded:reworded", "added:added"]);
    expect(kinds(true)).toEqual(["kept:unchanged", "reworded:reworded", "added:added", "removed:deleted"]);
  });
  it("Lists a claim tagged with several concerns under each of their groups", () => {
    const rows = [
      { id: "flow" },
      { id: "retry", concerns: ["idempotency", "auth"] },
      { id: "login", concerns: ["auth"] },
    ];
    const read = orderByConcern(rows, (r) => r.concerns, true).map(({ row, group }) => `${group ?? "-"}:${row.id}`);
    expect(read).toEqual(["auth:retry", "auth:login", "idempotency:retry", "-:flow"]);
    const edit = orderByConcern(rows, (r) => r.concerns, false).map(({ row }) => row.id);
    expect(edit).toEqual(["login", "retry", "flow"]);
  });
});
