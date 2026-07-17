/** Vite config for the throwaway DiagramView drag harness (see main.tsx).
 *  Root stays at the repo (like demo/vite.config.ts) so Tailwind v4's source
 *  scan covers src/ — the page is served at /harness/index.html. */
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: [
      {
        find: /^@tauri-apps\/api\/core$/,
        replacement: path.resolve(import.meta.dirname, "../demo/tauri-stub.ts"),
      },
      {
        find: /^@tauri-apps\/api\/event$/,
        replacement: path.resolve(import.meta.dirname, "../demo/tauri-stub.ts"),
      },
    ],
  },
  server: { port: 5188, strictPort: true },
});
