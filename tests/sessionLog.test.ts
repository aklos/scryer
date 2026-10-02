/**
 * The session log's pure joins (`src/features/session-log/session.ts`).
 *
 * What these pin: asks file under their prompt in log order and an unbroken
 * prompt is marked; the top-bar badge counts exactly open asks + untraced
 * files; plan keys resolve to names / statements and fall back to the raw key.
 */
import { describe, expect, it } from "vitest";
import {
  claimIndex,
  groupAsks,
  resolveKey,
  sessionBadge,
  type SessionAsk,
  type SessionView,
} from "../src/features/session-log/session";
import type { ScryModel } from "../src/entities/model/viewmodel";

const ask = (id: string, prompt: string, status: SessionAsk["status"]): SessionAsk => ({
  id,
  prompt,
  text: id,
  kind: "build",
  claims: [],
  status,
});

const view = (extra: Partial<SessionView> = {}): SessionView => ({
  session: "s1",
  startedAt: 0,
  updatedAt: 0,
  prompts: [
    { id: "p1", text: "first" },
    { id: "p2", text: "second" },
  ],
  unfiled: ["p2"],
  asks: [ask("a1", "p1", "delivered"), ask("a2", "p1", "open"), ask("a3", "p9", "open")],
  touched: [],
  untraced: ["x.ts", "y.ts"],
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

describe("groupAsks", () => {
  it("files asks under their prompt in order and marks unbroken prompts", () => {
    const g = groupAsks(view());
    expect(g.map((e) => [e.prompt.id, e.asks.map((a) => a.id), e.unfiled])).toEqual([
      ["p1", ["a1", "a2"], false],
      ["p2", [], true],
    ]);
  });
});

describe("sessionBadge", () => {
  it("counts open asks plus untraced files", () => {
    expect(sessionBadge(view())).toBe(4);
    expect(sessionBadge(view({ asks: [ask("a1", "p1", "descoped")], untraced: [] }))).toBe(0);
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
