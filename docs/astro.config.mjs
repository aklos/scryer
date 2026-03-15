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
              label: "Your First Model",
              slug: "getting-started/first-model",
            },
            {
              label: "Connecting AI Tools",
              slug: "getting-started/connecting-ai",
            },
          ],
        },
        {
          label: "The Editor",
          items: [
            { label: "Canvas & Navigation", slug: "editor/canvas" },
            { label: "Nodes", slug: "editor/nodes" },
            { label: "Edges & Relationships", slug: "editor/edges" },
            { label: "Properties Panel", slug: "editor/properties" },
          ],
        },
        {
          label: "Modeling",
          items: [
            { label: "Flows", slug: "modeling/flows" },
            { label: "Contracts", slug: "modeling/contracts" },
            { label: "Status Tracking", slug: "modeling/status" },
            { label: "Groups", slug: "modeling/groups" },
            { label: "Source Mapping", slug: "modeling/source-mapping" },
          ],
        },
        {
          label: "AI Integration",
          items: [
            { label: "MCP Server", slug: "ai/mcp-server" },
            {
              label: "Implementation Workflow",
              slug: "ai/implementation-workflow",
            },
            { label: "Drift & Sync", slug: "ai/drift-sync" },
            { label: "AI Advisor", slug: "ai/advisor" },
          ],
        },
        {
          label: "Customization",
          items: [{ label: "Themes", slug: "customization/themes" }],
        },
      ],
    }),
  ],
});
