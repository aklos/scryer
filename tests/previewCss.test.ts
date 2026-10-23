/**
 * Which stylesheets every preview gets: the global CSS the app's own entry
 * imports, found through the entry `index.html` actually loads.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { detectGlobalCss } from "../preview/plugin.mjs";

describe("detectGlobalCss", () => {
  it("When index.html loads an entry outside the usual paths, previews get the CSS that entry imports", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "scryer-preview-"));
    fs.mkdirSync(path.join(dir, "src", "app"), { recursive: true });
    fs.writeFileSync(path.join(dir, "index.html"), '<script type="module" src="/src/app/main.tsx"></script>');
    fs.writeFileSync(path.join(dir, "src", "app", "main.tsx"), 'import App from "./App";\nimport "./index.css";\n');
    expect(detectGlobalCss(dir)).toEqual(["/src/app/index.css"]);
  });
});
