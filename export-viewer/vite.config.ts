import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";
import { readFileSync } from "node:fs";
import path from "node:path";

/**
 * Export Vite config — builds the standalone HTML diagram viewer.
 *
 * Reuses the app's React + Tailwind setup but (a) aliases the Tauri backend
 * bridges to `tauri-stub.ts` so the lifted `DiagramView` renders in a plain
 * browser, (b) bakes a project's model into the bundle by replacing
 * `__SCRYER_MODEL_JSON__` with the contents of the `.scry` file named in
 * `SCRYER_MODEL_FILE`, and (c) collapses the whole build into one self-contained
 * `.html` (single file, all JS/CSS/fonts inlined) so it drops anywhere.
 *
 * Driven by `scripts/export-html.mjs`, not run by hand.
 */

const root = path.resolve(import.meta.dirname, "..");

// The model to bake in. The CLI sets SCRYER_MODEL_FILE to an absolute path; the
// file is the on-disk `.scry`, which is already a JSON-serialized ScryModel.
const modelFile = process.env.SCRYER_MODEL_FILE;
if (!modelFile) {
  throw new Error("SCRYER_MODEL_FILE is not set — run via scripts/export-html.mjs");
}
const modelJson = readFileSync(modelFile, "utf8");

export default defineConfig({
  root,
  plugins: [react(), tailwindcss(), viteSingleFile()],
  resolve: {
    alias: [
      { find: /^@tauri-apps\/api\/core$/, replacement: path.resolve(import.meta.dirname, "tauri-stub.ts") },
      { find: /^@tauri-apps\/api\/window$/, replacement: path.resolve(import.meta.dirname, "tauri-stub.ts") },
      { find: /^@tauri-apps\/api\/event$/, replacement: path.resolve(import.meta.dirname, "tauri-stub.ts") },
      { find: /^@tauri-apps\/plugin-dialog$/, replacement: path.resolve(import.meta.dirname, "tauri-stub.ts") },
    ],
  },
  define: {
    // Double-encoded: JSON.stringify of the file text yields a JS string literal
    // that main.tsx parses back into the model object at runtime.
    __SCRYER_MODEL_JSON__: JSON.stringify(modelJson),
  },
  build: {
    outDir: process.env.SCRYER_OUT_DIR || path.resolve(root, "dist-export"),
    emptyOutDir: true,
    // Inline every asset (fonts included) as a data URI so the output is truly
    // one file — viteSingleFile handles JS/CSS, this handles binaries.
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    rollupOptions: {
      input: path.resolve(import.meta.dirname, "index.html"),
    },
  },
});
