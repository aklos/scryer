/**
 * A preview server's config and cache choices (`preview/server.mjs`): a
 * project can hand previews their own vite config without touching its repo
 * root, and a preview server never shares a dependency-optimizer cache with
 * the dev server the project runs itself.
 */
import path from "node:path";
import { describe, expect, it } from "vitest";
import { previewCacheDir, previewConfigFile } from "../preview/server.mjs";

const project = "/repo";
const previewTs = path.join(project, ".scryer", "preview", "vite.config.ts");

/** `fs.existsSync` over a fixed set of paths. */
const on = (...paths: string[]) => (p: string) => paths.includes(p);

describe("previewConfigFile", () => {
  it("prefers the project's preview config over the package's own", () => {
    const own = path.join(project, "vite.config.ts");
    expect(previewConfigFile(project, project, on(previewTs, own))).toBe(previewTs);
  });

  it("serves a sub-package from the project-level preview config too", () => {
    const pkg = path.join(project, "apps", "web");
    expect(previewConfigFile(project, pkg, on(previewTs))).toBe(previewTs);
  });

  it("falls back to the package's own config when there is no preview config", () => {
    const own = path.join(project, "apps", "web", "vite.config.js");
    const pkg = path.join(project, "apps", "web");
    expect(previewConfigFile(project, pkg, on(own))).toBe(own);
  });

  it("is undefined when neither exists, leaving vite its own default search", () => {
    expect(previewConfigFile(project, project, on())).toBeUndefined();
  });
});

describe("previewCacheDir", () => {
  it("keeps the cache under .scryer/preview, not the package's node_modules", () => {
    const dir = previewCacheDir(project, project);
    expect(dir).toBe(path.join(project, ".scryer", "preview", ".vite", "root"));
    expect(dir).not.toContain("node_modules");
  });

  it("gives each package in a multi-package project its own slot", () => {
    const web = previewCacheDir(project, path.join(project, "apps", "web"));
    const ui = previewCacheDir(project, path.join(project, "packages", "ui"));
    expect(web).toBe(path.join(project, ".scryer", "preview", ".vite", "apps__web"));
    expect(web).not.toBe(ui);
  });

  it("hides the cache from the preview-content watcher (a dot-dir)", () => {
    const previewDir = path.join(project, ".scryer", "preview") + path.sep;
    const rel = previewCacheDir(project, project).slice(previewDir.length);
    expect(rel.startsWith(".")).toBe(true);
  });
});
