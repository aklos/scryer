/**
 * B3 — the shared preview server's Vite plugin.
 *
 * Serves any component of the target project as an instant virtual entry:
 *
 *   GET /__preview?file=src/Infobox.tsx&export=Infobox
 *
 * returns an HTML shell whose module script is a generated entry: import the
 * component, apply synthesized props (B2), auto-import the project's global
 * CSS, render into #root. Nothing is built per component — entries are
 * virtual modules on the always-running dev server.
 *
 * Also serves the spike harness:
 *   /__components.json  — discovered components + synthesized props
 *   /__harness          — page that renders every component in an iframe and
 *                         posts per-component results to /__report
 */

import fs from "node:fs";
import path from "node:path";
import { analyzeProject } from "./props.mjs";

const ENTRY_PREFIX = "/@scryer-preview/entry.js";

/**
 * @param {{ projectRoot: string, useWrapper?: boolean, onReport?: (results: any[]) => void }} opts
 */
export function scryerPreviewPlugin({ projectRoot, useWrapper = true, onReport }) {
  let analysis = null;
  const getAnalysis = () => (analysis ??= analyzeProject(projectRoot));

  const wrapperFile = path.join(projectRoot, ".scryer", "preview", "Wrapper.tsx");
  const hasWrapper = () => useWrapper && fs.existsSync(wrapperFile);

  /** Global CSS the app entry imports — auto-applied to every preview. */
  const globalCss = detectGlobalCss(projectRoot);

  return {
    name: "scryer-preview",

    resolveId(id) {
      if (id.startsWith(ENTRY_PREFIX)) return "\0" + id;
    },

    load(id) {
      if (!id.startsWith("\0" + ENTRY_PREFIX)) return;
      const params = new URLSearchParams(id.slice(id.indexOf("?") + 1));
      const file = params.get("file");
      const exportName = params.get("export") ?? "default";
      if (!file || file.includes("..")) return `throw new Error("bad preview entry");`;

      // Agent-written fixture (B5): realistic props that spread over the
      // synthesized defaults, when present.
      const fixtureRel = params.get("fixture");
      const fixture =
        fixtureRel && !fixtureRel.includes("..") && fs.existsSync(path.join(projectRoot, fixtureRel))
          ? "/" + fixtureRel.split(path.sep).join("/")
          : null;

      const comp = getAnalysis().components.find(
        (c) => c.file === file && c.exportName === exportName,
      );
      const propsCode = comp?.propsCode ?? "{}";
      // Shared, type-keyed fixtures (B6) referenced by the synthesized props —
      // imported under the tokens props.mjs baked into propsCode.
      const refImports = (comp?.fixtureRefs ?? [])
        .map((r) => `import { ${r.export} as ${r.token} } from ${JSON.stringify("/.scryer/preview/fixtures/" + r.module)};`)
        .join("\n");
      const cssImports = globalCss.map((c) => `import ${JSON.stringify(c)};`).join("\n");
      const wrapper = hasWrapper();

      return `
${wrapper ? `import Wrapper from "/.scryer/preview/Wrapper.tsx";` : ""}
${fixture ? `import __fixture from ${JSON.stringify(fixture)};` : ""}
${refImports}
import * as React from "react";
import { createRoot } from "react-dom/client";
${cssImports}
import * as Mod from ${JSON.stringify("/" + file)};

const meta = { file: ${JSON.stringify(file)}, exportName: ${JSON.stringify(exportName)} };
function report(status, error) {
  try { parent.postMessage({ type: "scryer-render", ...meta, status, error: error ?? null }, "*"); } catch {}
}

// Portal components render into document.body instead of #root — measure both.
const bodyBaseline = document.body.childElementCount;
function scheduleOkCheck() {
  requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(() => {
    const root = document.getElementById("root");
    const rendered = (root && root.childNodes.length > 0) || document.body.childElementCount > bodyBaseline;
    report(rendered ? "ok" : "empty");
  }, 120)));
}

const Component = Mod[${JSON.stringify(exportName)}];
const props = ${fixture ? `{ ...(${propsCode}), ...__fixture }` : `(${propsCode})`};
const wrap = (el) => ${wrapper ? `React.createElement(Wrapper, null, el)` : `el`};
let root = null;

// Optional children were injected for preview richness; components that
// spread props onto void DOM elements (e.g. <input>) reject them. Retry
// once without children before declaring the render failed.
let canRetryWithoutChildren = ${comp?.optionalChildrenInjected ? "true" : "false"};
window.addEventListener("error", (e) => {
  if (canRetryWithoutChildren && root) {
    canRetryWithoutChildren = false;
    const { children, ...rest } = props;
    try {
      root.render(wrap(React.createElement(Component, rest)));
      scheduleOkCheck();
      return;
    } catch {}
  }
  report("error", String(e.error?.stack ?? e.message));
});
window.addEventListener("unhandledrejection", (e) => report("error", "unhandled rejection: " + String(e.reason)));

if (!Component) {
  report("error", "export not found: " + meta.exportName);
} else {
  try {
    root = createRoot(document.getElementById("root"));
    root.render(wrap(React.createElement(Component, props)));
    scheduleOkCheck();
  } catch (err) {
    report("error", String(err?.stack ?? err));
  }
}
`;
    },

    configureServer(server) {
      // Entry modules bake in fixture/wrapper existence at generation time —
      // when anything under .scryer/preview changes (agent wrote a fixture, a
      // wrapper, or variant files), regenerate the entries and reload open
      // previews.
      const previewDir = path.join(projectRoot, ".scryer", "preview") + path.sep;
      server.watcher.on("all", (_event, file) => {
        if (!file.startsWith(previewDir)) return;
        // A changed manifest (or shared fixture) changes which props.mjs
        // emits — drop the memoized analysis so entries re-synthesize.
        analysis = null;
        for (const mod of server.moduleGraph.idToModuleMap.values()) {
          if (mod.id?.startsWith("\0" + ENTRY_PREFIX)) {
            server.moduleGraph.invalidateModule(mod);
          }
        }
        server.ws.send({ type: "full-reload" });
      });

      server.middlewares.use(async (req, res, next) => {
        const url = new URL(req.url, "http://localhost");

        if (url.pathname === "/__preview") {
          const file = url.searchParams.get("file") ?? "";
          const exportName = url.searchParams.get("export") ?? "default";
          const dark = url.searchParams.get("theme") === "dark";
          const bg = sanitizeColor(url.searchParams.get("bg"));
          const fg = sanitizeColor(url.searchParams.get("fg"));
          const entryParams = new URLSearchParams({ file, export: exportName });
          const fixture = url.searchParams.get("fixture");
          if (fixture) entryParams.set("fixture", fixture);
          const entry = `${ENTRY_PREFIX}?${entryParams}`;
          const html = await server.transformIndexHtml(req.url, previewHtml(entry, dark, bg, fg));
          res.setHeader("Content-Type", "text/html");
          res.end(html);
          return;
        }

        if (url.pathname === "/__components.json") {
          let body;
          try {
            body = JSON.stringify(getAnalysis(), null, 2);
          } catch (err) {
            res.statusCode = 500;
            res.end(JSON.stringify({ error: String(err?.stack ?? err) }));
            return;
          }
          res.setHeader("Content-Type", "application/json");
          res.end(body);
          return;
        }

        if (url.pathname === "/__harness") {
          res.setHeader("Content-Type", "text/html");
          res.end(harnessHtml());
          return;
        }

        if (url.pathname === "/__report" && req.method === "POST") {
          const chunks = [];
          for await (const chunk of req) chunks.push(chunk);
          const results = JSON.parse(Buffer.concat(chunks).toString());
          res.end("ok");
          onReport?.(results);
          return;
        }

        next();
      });
    },
  };
}

/** CSS files imported by the app entry (main.tsx etc.), as root-relative paths. */
function detectGlobalCss(projectRoot) {
  for (const entry of ["src/main.tsx", "src/main.jsx", "src/index.tsx", "src/main.ts"]) {
    const full = path.join(projectRoot, entry);
    if (!fs.existsSync(full)) continue;
    const source = fs.readFileSync(full, "utf8");
    const css = [];
    for (const m of source.matchAll(/import\s+["']([^"']+\.css)["']/g)) {
      const resolved = m[1].startsWith(".")
        ? "/" + path.posix.join(path.posix.dirname(entry), m[1])
        : m[1]; // bare package import — leave for Vite to resolve
      css.push(resolved);
    }
    return css;
  }
  return [];
}

/** Accept only simple hex/rgb(a) color literals from the URL — these land
 *  inside an inline <style>, so reject anything that could break out of it. */
function sanitizeColor(value) {
  if (!value) return null;
  return /^#[0-9a-fA-F]{3,8}$|^rgba?\([\d.,\s%/]+\)$/.test(value) ? value : null;
}

function previewHtml(entrySrc, dark = false, bg = null, fg = null) {
  // Fall back to the caller's resolved theme colors so the first paint matches
  // scryer's chrome; only then to neutral light/dark defaults. The project's
  // own --surface-canvas (loaded async via the CSS module) still wins once ready.
  const bgFallback = bg ?? (dark ? "#0a0a0a" : "#ffffff");
  const fgFallback = fg ?? (dark ? "#f5f5f5" : "#111111");
  return `<!doctype html>
<html${dark ? ` class="dark"` : ""}>
  <head>
    <meta charset="utf-8" />
    <style>html, body { margin: 0; } body { padding: 12px; background: var(--surface-canvas, ${bgFallback}); color: var(--text, ${fgFallback}); }</style>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="${entrySrc}"></script>
  </body>
</html>`;
}

function harnessHtml() {
  return `<!doctype html>
<html>
  <head><meta charset="utf-8"><title>scryer preview harness</title></head>
  <body>
    <pre id="log"></pre>
    <div id="stage" style="width:900px;height:600px"></div>
    <script>
      // The first component pays the server's cold start (dep optimization,
      // Tailwind, the bulk of the module graph) — give it a bigger budget.
      const COLD_TIMEOUT_MS = 40000, TIMEOUT_MS = 8000;
      const log = (line) => { document.getElementById("log").textContent += line + "\\n"; };

      async function run() {
        const { components } = await (await fetch("/__components.json")).json();
        const results = [];
        for (const [i, comp] of components.entries()) {
          const result = await renderOne(comp, i === 0 ? COLD_TIMEOUT_MS : TIMEOUT_MS);
          results.push(result);
          log(result.status.padEnd(8) + comp.file + ":" + comp.exportName + (result.error ? "  " + result.error.split("\\n")[0] : ""));
        }
        await fetch("/__report", { method: "POST", body: JSON.stringify(results) });
        log("DONE");
        document.title = "scryer-harness-done";
      }

      function renderOne(comp, timeoutMs) {
        return new Promise((resolve) => {
          const iframe = document.createElement("iframe");
          iframe.style.cssText = "width:100%;height:100%;border:0";
          let settled = false;
          const t0 = performance.now();
          const finish = (status, error) => {
            if (settled) return; // first report wins
            settled = true;
            cleanup();
            resolve({ file: comp.file, exportName: comp.exportName, warnings: comp.warnings, status, error: error ?? null, ms: Math.round(performance.now() - t0) });
          };
          const onMessage = (e) => {
            const d = e.data;
            if (!d || d.type !== "scryer-render" || d.file !== comp.file || d.exportName !== comp.exportName) return;
            finish(d.status, d.error);
          };
          const timer = setTimeout(() => finish("timeout", null), timeoutMs);
          const cleanup = () => { clearTimeout(timer); window.removeEventListener("message", onMessage); setTimeout(() => iframe.remove(), 0); };
          window.addEventListener("message", onMessage);
          iframe.src = "/__preview?" + new URLSearchParams({ file: comp.file, export: comp.exportName });
          document.getElementById("stage").appendChild(iframe);
        });
      }

      run().catch((err) => { log("HARNESS ERROR: " + err); document.title = "scryer-harness-done"; });
    </script>
  </body>
</html>`;
}
