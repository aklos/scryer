/**
 * Vue single-file component discovery (`preview/props.mjs`): every `.vue`
 * under a package that depends on `vue` is a previewable default export named
 * after its file; dependency/build/tool directories are skipped; a package
 * without `vue` yields nothing, however many `.vue` files it holds.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { discoverVueComponents, vueComponentName } from "../preview/props.mjs";

let root: string;
const write = (rel: string, text = "") => {
  const abs = path.join(root, rel);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, text);
};

beforeEach(() => {
  root = fs.mkdtempSync(path.join(os.tmpdir(), "scryer-vue-"));
});
afterEach(() => fs.rmSync(root, { recursive: true, force: true }));

describe("vueComponentName", () => {
  it("resolves a file to the PascalCase name Vue gives it", () => {
    expect(vueComponentName("src/user-card.vue")).toBe("UserCard");
    expect(vueComponentName("user_card.vue")).toBe("UserCard");
    expect(vueComponentName("UserCard.vue")).toBe("UserCard");
  });
});

describe("discoverVueComponents", () => {
  it("lists every SFC as a default-export component named after its file", () => {
    write("package.json", JSON.stringify({ dependencies: { vue: "^3" } }));
    write("src/components/user-card.vue", "<template/>");
    write("src/App.vue", "<template/>");
    const found = discoverVueComponents(root)
      .map((c) => [c.file, c.displayName, c.exportName, c.framework])
      .sort();
    expect(found).toEqual([
      ["src/App.vue", "App", "default", "vue"],
      ["src/components/user-card.vue", "UserCard", "default", "vue"],
    ]);
  });

  it("skips dependency, build, and tool output directories", () => {
    write("package.json", JSON.stringify({ devDependencies: { vue: "^3" } }));
    write("node_modules/lib/Thing.vue");
    write("dist/Built.vue");
    write(".scryer/preview/Wrapper.vue");
    write("src/Real.vue");
    expect(discoverVueComponents(root).map((c) => c.file)).toEqual(["src/Real.vue"]);
  });

  it("yields nothing for a package that does not depend on vue", () => {
    write("package.json", JSON.stringify({ dependencies: { react: "^18" } }));
    write("src/Stray.vue");
    expect(discoverVueComponents(root)).toEqual([]);
  });

  it("reports files relative to the project root when the package is nested", () => {
    write("apps/web/package.json", JSON.stringify({ dependencies: { vue: "^3" } }));
    write("apps/web/src/Page.vue");
    expect(discoverVueComponents(root, path.join(root, "apps/web")).map((c) => c.file)).toEqual([
      "apps/web/src/Page.vue",
    ]);
  });
});
