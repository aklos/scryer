import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

/**
 * Demo / trailer Vite config. Reuses the app's React + Tailwind setup but
 * aliases the Tauri backend bridges to harmless stubs (`tauri-stub.ts`) so
 * lifted components render in a plain browser. Kept separate from the app's
 * `vite.config.ts` so the product build never sees these aliases.
 *
 *   pnpm demo   →   http://localhost:5199/demo/index.html
 */
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: [
      { find: /^@tauri-apps\/api\/core$/, replacement: path.resolve(import.meta.dirname, "tauri-stub.ts") },
      { find: /^@tauri-apps\/api\/window$/, replacement: path.resolve(import.meta.dirname, "tauri-stub.ts") },
    ],
  },
  server: {
    port: 5199,
    strictPort: true,
  },
});
