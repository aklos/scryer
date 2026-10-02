/**
 * The session log's pure joins (`src/features/session-log/session.ts`).
 *
 * What these pin: the top-bar badge counts exactly the plan entries left
 * unbuilt; plan keys resolve to names / statements and fall back to the raw key.
 */
import { describe, expect, it } from "vitest";
import { claimIndex, resolveKey, sessionBadge, type SessionView } from "../src/features/session-log/session";
import type { ScryModel } from "../src/entities/model/viewmodel";

const view = (extra: Partial<SessionView> = {}): SessionView => ({
  session: "s1",
  startedAt: 0,
  updatedAt: 0,
  touched: [],
  unfolded: [
    { key: "resp:r1", label: "posts entries", note: "built; reversal left" },
    { key: "resp:r2", label: "owns money" },
  ],
  modelEdits: [],
  ...extra,
});

const model: ScryModel = {
  version: "0.3",
  nodes: [
    { id: "n1", kind: "component", name: "Ledger", responsibilities: [{ id: "r1", statement: "posts entries" }] } as ScryModel["nodes"][number],
    { id: "n2", kind: "component", name: "Bank" } as ScryModel["nodes"][number],
  ],
  links: [{ id: "l1", src: "n1", dst: "n2", label: "settles" }],
  groups: [{ id: "g1", name: "Core", memberIds: [], responsibilities: [{ id: "r2", statement: "owns money" }] } as unknown as ScryModel["groups"][number]],
};

describe("sessionBadge", () => {
  it("counts the plan entries left unbuilt", () => {
    expect(sessionBadge(view())).toBe(2);
    expect(sessionBadge(view({ unfolded: [] }))).toBe(0);
    expect(sessionBadge(null)).toBe(0);
  });
});

describe("resolveKey", () => {
  const claims = claimIndex(model);
  it("resolves claims with their host, nodes, groups, links and props", () => {
    expect(resolveKey("resp:r1", model, claims)).toMatchObject({ label: "posts entries", hostKind: "node", hostId: "n1", respId: "r1" });
    expect(resolveKey("resp:r2", model, claims)).toMatchObject({ hostKind: "group", hostId: "g1", hostName: "Core" });
    expect(resolveKey("node:n2", model, claims)).toMatchObject({ label: "Bank", hostId: "n2" });
    expect(resolveKey("group:g1", model, claims)).toMatchObject({ label: "Core", hostKind: "group" });
    expect(resolveKey("link:l1", model, claims).label).toBe("Ledger → Bank");
    expect(resolveKey("prop:n1:amount:cents", model, claims).label).toBe("Ledger.amount:cents");
  });
  it("falls back to the raw key", () => {
    expect(resolveKey("resp:gone", model, claims)).toEqual({ key: "resp:gone", kind: "resp", label: "resp:gone" });
    expect(resolveKey("weird", model, claims).label).toBe("weird");
  });
});
