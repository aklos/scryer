/**
 * The preview router's ownership rule (`preview/server.mjs`): in a
 * multi-package project each component is served from the sub-package that
 * owns it — the nearest enclosing package wins, and files outside every
 * package fall back to the first one.
 */
import { describe, expect, it } from "vitest";
import { ownerOf } from "../preview/server.mjs";

const pkgs = [
  { rel: "apps", url: "http://p1" },
  { rel: "apps/web", url: "http://p2" },
  { rel: "packages/ui", url: "http://p3" },
];

describe("ownerOf", () => {
  it("serves a file from the sub-package that owns it", () => {
    expect(ownerOf(pkgs, "packages/ui/src/Button.tsx").rel).toBe("packages/ui");
    expect(ownerOf(pkgs, "apps/src/Page.tsx").rel).toBe("apps");
  });

  it("prefers the nearest enclosing package over a broader one", () => {
    expect(ownerOf(pkgs, "apps/web/src/App.tsx").rel).toBe("apps/web");
  });

  it("does not mistake a name prefix for enclosure", () => {
    expect(ownerOf(pkgs, "apps/webby/src/App.tsx").rel).toBe("apps");
  });

  it("falls back to the first package for files outside every one", () => {
    expect(ownerOf(pkgs, "scripts/build.mjs").rel).toBe("apps");
  });
});
