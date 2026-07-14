/**
 * The TS mirrors of Rust's concern machinery (`crates/scryer-core/src/concerns.rs`)
 * — `registerConcerns` runs at the canvas write chokepoint exactly as
 * `register_concerns` runs in the Rust write paths, so the two must agree on
 * normalization, minting, and no-prune semantics. Keep in lockstep with the
 * Rust unit tests in that module.
 */
import { describe, expect, it } from "vitest";
import {
  concernCounts,
  emptyModel,
  normalizeConcernSlug,
  registerConcerns,
  renameConcern,
  type Node,
  type Responsibility,
  type ScryModel,
} from "../src/viewmodel";

let respSeq = 0;
const resp = (concern?: string): Responsibility => ({
  id: `resp-${++respSeq}`,
  statement: "does a thing",
  ...(concern !== undefined ? { concern } : {}),
});

const node = (id: string, parentId: string | undefined, resps: Responsibility[]): Node => ({
  id,
  kind: "component",
  name: id,
  ...(parentId ? { parentId } : {}),
  responsibilities: resps,
});

const modelWith = (nodes: Node[]): ScryModel => ({ ...emptyModel(), nodes });

describe("normalizeConcernSlug", () => {
  it("kebabs and collapses like the Rust normalize_slug", () => {
    expect(normalizeConcernSlug("Auth")).toBe("auth");
    expect(normalizeConcernSlug("Failure  Handling")).toBe("failure-handling");
    expect(normalizeConcernSlug("--rate__limiting--")).toBe("rate-limiting");
    expect(normalizeConcernSlug("  ")).toBe("");
  });
});

describe("registerConcerns", () => {
  it("mints registry entries, seeds standards, normalizes tags, clears empties", () => {
    const m = registerConcerns(
      modelWith([node("n1", undefined, [resp("Auth"), resp("session-windows"), resp(), resp("!!")])]),
    );
    const rs = m.nodes[0].responsibilities!;
    expect(rs[0].concern).toBe("auth");
    expect(rs[3].concern).toBeUndefined();
    const auth = m.concerns!.find((c) => c.slug === "auth")!;
    expect(auth.icon).toBe("Shield");
    expect(auth.description).toBeTruthy();
    const custom = m.concerns!.find((c) => c.slug === "session-windows")!;
    expect(custom.icon).toBeUndefined();
    expect(m.concerns!.map((c) => c.slug)).toEqual(["auth", "session-windows"]);
  });

  it("never touches existing entries or prunes unused ones", () => {
    const base = modelWith([node("n1", undefined, [resp("auth")])]);
    base.concerns = [
      { slug: "auth", description: "user-curated wording", icon: "Lock" },
      { slug: "unused" },
    ];
    const m = registerConcerns(base);
    // Nothing to change → same reference (the chokepoint stays cheap).
    expect(m).toBe(base);
    expect(m.concerns!.find((c) => c.slug === "auth")!.icon).toBe("Lock");
    expect(m.concerns!.some((c) => c.slug === "unused")).toBe(true);
  });
});

describe("renameConcern", () => {
  it("rewrites the registry entry and every tagged responsibility in one step", () => {
    const base = registerConcerns(
      modelWith([node("n1", undefined, [resp("auth"), resp("auth"), resp("persistence")])]),
    );
    const m = renameConcern(base, "auth", "Access Control");
    const rs = m.nodes[0].responsibilities!;
    expect(rs.filter((r) => r.concern === "access-control")).toHaveLength(2);
    expect(m.concerns!.some((c) => c.slug === "auth")).toBe(false);
    // The renamed entry carries the old decoration to the new slug.
    expect(m.concerns!.find((c) => c.slug === "access-control")!.icon).toBe("Shield");
  });

  it("merging onto an existing slug keeps the established entry", () => {
    const base = registerConcerns(
      modelWith([node("n1", undefined, [resp("auth"), resp("access-control")])]),
    );
    const m = renameConcern(base, "auth", "access-control");
    expect(m.nodes[0].responsibilities!.every((r) => r.concern === "access-control")).toBe(true);
    const entries = m.concerns!.filter((c) => c.slug === "access-control");
    expect(entries).toHaveLength(1);
    expect(entries[0].icon).toBeUndefined(); // the target entry, not auth's Shield
  });

  it("blank or identical target is a no-op", () => {
    const base = registerConcerns(modelWith([node("n1", undefined, [resp("auth")])]));
    expect(renameConcern(base, "auth", "  ")).toBe(base);
    expect(renameConcern(base, "auth", "auth")).toBe(base);
  });
});

describe("concernCounts", () => {
  it("rolls own counts up the ancestor chain; absent id means unlit", () => {
    const m = modelWith([
      node("sys", undefined, []),
      node("api", "sys", [resp("auth")]),
      node("webhook", "api", [resp("auth"), resp("idempotency")]),
      node("ui", "sys", []),
    ]);
    const counts = concernCounts(m, "auth");
    expect(counts.get("webhook")).toBe(1);
    expect(counts.get("api")).toBe(2);
    expect(counts.get("sys")).toBe(2);
    expect(counts.has("ui")).toBe(false);
  });
});
