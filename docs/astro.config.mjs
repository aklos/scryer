import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import react from "@astrojs/react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  site: "https://aklos.github.io",
  base: "/scryer",
  vite: {
    plugins: [tailwindcss()],
    resolve: {
      alias: {
        // Map @app/* to the main app's src/ so doc components can import
        // shapes, types, colors, etc. directly from the app source.
        "@app": new URL("../src", import.meta.url).pathname,
        // Stub Tauri APIs so we can import app source files that transitively
        // reference @tauri-apps/* without pulling in the desktop runtime.
        "@tauri-apps/api/window": new URL(
          "./src/stubs/tauri.ts",
          import.meta.url
        ).pathname,
        "@tauri-apps/api/core": new URL(
          "./src/stubs/tauri.ts",
          import.meta.url
        ).pathname,
        "@xyflow/react": new URL(
          "./src/stubs/xyflow.tsx",
          import.meta.url
        ).pathname,
      },
    },
  },
  redirects: {
    "/": "/scryer/welcome/",
  },
  integrations: [
    react(),
    starlight({
      title: "scryer",
      favicon: "/favicon.png",
      logo: {
        src: "./public/logo.png",
      },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/aklos/scryer",
        },
      ],
      customCss: ["./src/styles/scryer.css"],
      sidebar: [
        { label: "Welcome", slug: "welcome" },
        {
          label: "Getting Started",
          items: [
            { label: "Installation", slug: "getting-started/installation" },
            {
              label: "Connecting AI Tools",
              slug: "getting-started/connecting-ai",
            },
          ],
        },
        {
          label: "Concepts",
          items: [
            { label: "Claims", slug: "concepts/claims" },
            { label: "Status", slug: "concepts/status" },
            { label: "Structure", slug: "concepts/structure" },
          ],
        },
        {
          label: "AI Integration",
          items: [{ label: "MCP Server", slug: "ai/mcp-server" }],
        },
        {
          label: "Customization",
          items: [{ label: "Themes", slug: "customization/themes" }],
        },
      ],
    }),
  ],
});
