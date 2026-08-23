/**
 * B3 — one shared Vite dev server per project.
 *
 * Boots Vite out of the *target project's* own node_modules, reusing the
 * project's vite.config so its framework plugins (react, tailwind, aliases)
 * apply to previews unchanged. Our preview plugin rides on top.
 *
 * When the project root itself can't resolve `vite` (a monorepo whose vite
 * app lives in a sub-package, e.g. a Next.js root with a `web-embed/` vite
 * package), the nearest-package rule applies: every sub-package that resolves
 * vite from its own package.json gets a vite server rooted there, and a tiny
 * front router — the single reported URL — merges `/__components.json` and
 * 302s each `/__preview` to the package that owns its `file`. All file params
 * stay project-root-relative throughout; only import URLs inside the plugin
 * know which package a server is rooted at.
 *
 * CLI: node preview/server.mjs [projectRoot] [--port N] [--no-wrapper]
 */

import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { scryerPreviewPlugin } from "./plugin.mjs";

function resolvesVite(dir) {
  try {
    createRequire(path.join(dir, "package.json")).resolve("vite");
    return true;
  } catch {
    return false;
  }
}

/**
 * Directories to boot vite servers in: the project root when it resolves vite
 * (the common case), else every sub-package that does. Doesn't descend into a
 * found vite package (nearest-to-root wins) or into dependency/build dirs.
 */
function findViteRoots(projectRoot, maxDepth = 4) {
  if (resolvesVite(projectRoot)) return [projectRoot];
  const roots = [];
  const skip = new Set(["node_modules", "dist", "build", "out", "target"]);
  (function walk(dir, depth) {
    if (depth > maxDepth) return;
    let entries = [];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (!e.isDirectory() || e.name.startsWith(".") || skip.has(e.name)) continue;
      const child = path.join(dir, e.name);
      if (fs.existsSync(path.join(child, "package.json")) && resolvesVite(child)) {
        roots.push(child);
        continue;
      }
      walk(child, depth + 1);
    }
  })(projectRoot, 1);
  return roots;
}

/** One vite dev server rooted at `packageRoot`, previewing in project-relative
 *  path space (see plugin.mjs). */
async function startViteFor({ projectRoot, packageRoot, port, useWrapper }) {
  const require = createRequire(path.join(packageRoot, "package.json"));
  let vite = await import(pathToFileURL(require.resolve("vite")).href);
  // require.resolve can land on vite 5's CJS build, whose API surfaces as the
  // ESM default export rather than named exports.
  if (!vite.createServer && vite.default?.createServer) vite = vite.default;

  const configFile = ["vite.config.ts", "vite.config.js", "vite.config.mjs"]
    .map((f) => path.join(packageRoot, f))
    .find(fs.existsSync);

  const searchForWorkspaceRoot = vite.searchForWorkspaceRoot ?? ((dir) => dir);

  const server = await vite.createServer({
    configFile,
    root: packageRoot,
    clearScreen: false,
    server: {
      port,
      strictPort: false,
      host: "127.0.0.1",
      open: false,
      // Fixtures/wrapper/variations live at {project}/.scryer/preview — when
      // the vite root is a sub-package they're served via /@fs/, which needs
      // the project root allowed.
      ...(packageRoot !== projectRoot
        ? { fs: { allow: [searchForWorkspaceRoot(packageRoot), projectRoot] } }
        : {}),
    },
    plugins: [scryerPreviewPlugin({ projectRoot, packageRoot, useWrapper })],
  });
  await server.listen();

  const url = server.resolvedUrls?.local?.[0]?.replace(/\/$/, "")
    ?? `http://127.0.0.1:${server.config.server.port}`;
  return { server, url };
}

function listenOnFreePort(server, startPort) {
  return new Promise((resolve, reject) => {
    let port = startPort;
    const tryListen = () => {
      server.once("error", (err) => {
        if (err.code === "EADDRINUSE" && port < startPort + 100) {
          port += 1;
          tryListen();
        } else {
          reject(err);
        }
      });
      server.listen(port, "127.0.0.1", () => {
        server.removeAllListeners("error");
        resolve(`http://127.0.0.1:${port}`);
      });
    };
    tryListen();
  });
}

/** The sub-package that owns a project-relative file: the nearest (deepest)
 *  package whose directory encloses it, falling back to the first package for
 *  files outside every one. Exported for tests. */
export function ownerOf(pkgs, file) {
  // Deepest package first, so the nearest enclosing package owns a file.
  const byDepth = [...pkgs].sort((a, b) => b.rel.length - a.rel.length);
  return byDepth.find((p) => file === p.rel || file.startsWith(p.rel + "/")) ?? pkgs[0];
}

/** The front router for multi-package projects: merges component lists and
 *  redirects previews to the package server owning the requested file. */
async function startRouter({ pkgs, port }) {

  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, "http://localhost");

    if (url.pathname === "/__components.json") {
      res.setHeader("Access-Control-Allow-Origin", "*");
      res.setHeader("Content-Type", "application/json");
      try {
        const lists = await Promise.all(
          pkgs.map(async (p) => {
            const body = await (await fetch(p.url + "/__components.json")).json();
            if (body.error) throw new Error(`${p.rel}: ${body.error}`);
            return body.components ?? [];
          }),
        );
        const components = lists
          .flat()
          .sort((a, b) => a.file.localeCompare(b.file) || a.exportName.localeCompare(b.exportName));
        res.end(JSON.stringify({ components }, null, 2));
      } catch (err) {
        res.statusCode = 500;
        res.end(JSON.stringify({ error: String(err?.stack ?? err) }));
      }
      return;
    }

    if (url.pathname === "/__preview") {
      const target = ownerOf(pkgs, url.searchParams.get("file") ?? "");
      res.statusCode = 302;
      res.setHeader("Location", target.url + "/__preview" + url.search);
      res.end();
      return;
    }

    res.statusCode = 404;
    res.end("scryer preview router: unknown path");
  });

  const url = await listenOnFreePort(server, port);
  return { server, url };
}

/**
 * @param {{ projectRoot: string, port?: number, useWrapper?: boolean }} opts
 * @returns {Promise<{ server: { close(): Promise<void> }, url: string }>}
 */
export async function startPreviewServer({ projectRoot, port = 4848, useWrapper = true }) {
  projectRoot = path.resolve(projectRoot);
  const roots = findViteRoots(projectRoot);
  if (roots.length === 0) {
    throw new Error(
      `no package resolving 'vite' found in ${projectRoot} or its sub-packages — ` +
        `previews need a vite-based web app (install vite in the package that owns the components)`,
    );
  }

  if (roots.length === 1 && roots[0] === projectRoot) {
    return startViteFor({ projectRoot, packageRoot: projectRoot, port, useWrapper });
  }

  const pkgs = [];
  const servers = [];
  for (const root of roots) {
    const { server, url } = await startViteFor({ projectRoot, packageRoot: root, port, useWrapper });
    servers.push(server);
    pkgs.push({ root, rel: path.relative(projectRoot, root).split(path.sep).join("/"), url });
  }
  const { server: router, url } = await startRouter({ pkgs, port });
  const close = async () => {
    router.close();
    await Promise.all(servers.map((s) => s.close()));
  };
  return { server: { close }, url };
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

  // Tie our lifetime to the host process: when it exits (or is killed), the
  // stdin pipe closes and we shut down instead of orphaning.
  if (args.includes("--exit-on-stdin-close")) {
    process.stdin.resume();
    process.stdin.on("end", () => process.exit(0));
    process.stdin.on("error", () => process.exit(0));
  }
}
