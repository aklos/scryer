/**
 * B3 — one shared Vite dev server per project.
 *
 * Boots Vite out of the *target project's* own node_modules, reusing the
 * project's vite.config so its framework plugins (react, tailwind, aliases)
 * apply to previews unchanged. Our preview plugin rides on top.
 *
 * CLI: node preview/server.mjs [projectRoot] [--port N] [--no-wrapper]
 */

import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import fs from "node:fs";
import path from "node:path";
import { scryerPreviewPlugin } from "./plugin.mjs";

/**
 * @param {{ projectRoot: string, port?: number, useWrapper?: boolean, onReport?: (results: any[]) => void }} opts
 * @returns {Promise<{ server: import("vite").ViteDevServer, url: string }>}
 */
export async function startPreviewServer({ projectRoot, port = 4848, useWrapper = true, onReport }) {
  projectRoot = path.resolve(projectRoot);
  const require = createRequire(path.join(projectRoot, "package.json"));
  const vite = await import(pathToFileURL(require.resolve("vite")).href);

  const configFile = ["vite.config.ts", "vite.config.js", "vite.config.mjs"]
    .map((f) => path.join(projectRoot, f))
    .find(fs.existsSync);

  const server = await vite.createServer({
    configFile,
    root: projectRoot,
    clearScreen: false,
    server: { port, strictPort: false, host: "127.0.0.1", open: false },
    plugins: [scryerPreviewPlugin({ projectRoot, useWrapper, onReport })],
  });
  await server.listen();

  const url = server.resolvedUrls?.local?.[0]?.replace(/\/$/, "")
    ?? `http://127.0.0.1:${server.config.server.port}`;
  return { server, url };
}

// CLI mode
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  const positional = args.filter((a) => !a.startsWith("--"));
  const portArg = args.find((a) => a.startsWith("--port"));
  const { url } = await startPreviewServer({
    projectRoot: positional[0] ?? process.cwd(),
    port: portArg ? Number(portArg.split("=")[1] ?? args[args.indexOf(portArg) + 1]) : 4848,
    useWrapper: !args.includes("--no-wrapper"),
  });
  // Machine-parseable first line — the Tauri host reads this to learn the port.
  console.log(`SCRYER_PREVIEW_URL=${url}`);
  console.log(`  components: ${url}/__components.json`);
  console.log(`  preview:    ${url}/__preview?file=src/Foo.tsx&export=Foo`);
  console.log(`  harness:    ${url}/__harness`);

  // Tie our lifetime to the host process: when it exits (or is killed), the
  // stdin pipe closes and we shut down instead of orphaning.
  if (args.includes("--exit-on-stdin-close")) {
    process.stdin.resume();
    process.stdin.on("end", () => process.exit(0));
    process.stdin.on("error", () => process.exit(0));
  }
}
