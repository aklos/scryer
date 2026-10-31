/** Todo claims for `ClaimsSection`. */
import { describe, expect, it } from "vitest";
import { orderByConcern } from "../src/widgets/node-page-sections/ClaimsSection";

describe("ClaimsSection", () => {
  it.todo("Lists a node's claims in concern groups ordered A to Z, with the untagged core flow after them in its own order");
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
