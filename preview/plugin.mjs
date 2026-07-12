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
 * Also serves /__components.json — discovered components + synthesized props.
 */

import fs from "node:fs";
import path from "node:path";
import { analyzeProject } from "./props.mjs";

const ENTRY_PREFIX = "/@scryer-preview/entry.js";

/**
 * @param {{ projectRoot: string, useWrapper?: boolean }} opts
 */
export function scryerPreviewPlugin({ projectRoot, useWrapper = true }) {
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
// Whether this render has real data behind it: an agent-written per-node
// fixture, or any type-keyed shared fixture matched by the component's props.
// Drives the "Generate preview data" affordance even when the render is "ok".
const __hasFixture = ${fixture ? "true" : comp?.fixtureRefs?.length ? "true" : "false"};
function report(status, error) {
  try { parent.postMessage({ type: "scryer-render", ...meta, status, error: error ?? null, hasFixture: __hasFixture }, "*"); } catch {}
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
          // Two independent themes: `theme` is the previewed COMPONENT's
          // theme (a best-effort .dark class on the iframe — see previewHtml);
          // `canvas` is scryer's chrome theme, which drives the checkerboard
          // backdrop only. They are decoupled on purpose.
          const componentDark = url.searchParams.get("theme") === "dark";
          const canvasDark = url.searchParams.get("canvas") === "dark";
          const entryParams = new URLSearchParams({ file, export: exportName });
          const fixture = url.searchParams.get("fixture");
          if (fixture) entryParams.set("fixture", fixture);
          const entry = `${ENTRY_PREFIX}?${entryParams}`;
          const html = await server.transformIndexHtml(req.url, previewHtml(entry, componentDark, canvasDark));
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

// `componentDark` themes the previewed component (best-effort: a .dark class +
// color-scheme on the iframe — works for class-based theming like Tailwind, a
// no-op for prefers-color-scheme / provider-based theming we can't reach from an
// isolated iframe). `canvasDark` is scryer's chrome theme and drives ONLY the
// checkerboard backdrop, so the two stay decoupled.
function previewHtml(entrySrc, componentDark = false, canvasDark = false) {
  const fgFallback = componentDark ? "#f5f5f5" : "#111111";
  // The component mounts into #root (entry contract unchanged); #root is nested
  // in a transform layer (#stage) inside a clipped viewport (#canvas). The
  // viewport paints a low-contrast transparency checkerboard so the component
  // reads as clearly separate from it. A small harness centers the component at
  // 100%; when (and only when) it is larger than the pane you can zoom OUT
  // (never above 100%, anchored to the centre so it stays centred) and pan.
  // Variation thumbnails set pointer-events:none on the iframe element, so they
  // get the static centred view only.
  // Two near-equal greys per theme — subtle texture, not a loud checker.
  const checkA = canvasDark ? "#202020" : "#dcdcdc";
  const checkB = canvasDark ? "#181818" : "#f1f1f1";
  return `<!doctype html>
<html${componentDark ? ` class="dark"` : ""}>
  <head>
    <meta charset="utf-8" />
    <style>
      html, body { margin: 0; height: 100%; }
      html { color-scheme: ${componentDark ? "dark" : "light"}; }
      body { color: var(--text, ${fgFallback}); overflow: hidden; }
      #canvas {
        position: fixed; inset: 0; overflow: hidden;
        background-color: ${checkB};
        background-image: repeating-conic-gradient(${checkA} 0% 25%, ${checkB} 0% 50%);
        background-size: 16px 16px;
        cursor: default;
      }
      #canvas.pannable { cursor: grab; }
      #canvas.grabbing { cursor: grabbing; }
      #stage { position: absolute; top: 0; left: 0; transform-origin: 0 0; will-change: transform; }
      #root { display: inline-block; }
    </style>
  </head>
  <body>
    <div id="canvas"><div id="stage"><div id="root"></div></div></div>
    <script type="module" src="${entrySrc}"></script>
    <script>
      (function () {
        var canvas = document.getElementById("canvas");
        var stage = document.getElementById("stage");
        var root = document.getElementById("root");
        var FIT_PAD = 24;
        var scale = 1, tx = 0, ty = 0, touched = false;

        function dims() {
          return { cw: canvas.clientWidth, ch: canvas.clientHeight, rw: root.scrollWidth, rh: root.scrollHeight };
        }
        // The component is larger than the pane at its natural size — the sole
        // case where zooming out (and panning) is meaningful.
        function overflowsNatural() {
          var d = dims();
          return d.rw > d.cw || d.rh > d.ch;
        }
        // Smallest scale we allow: just enough to fit with a margin (never up).
        function fitScale() {
          var d = dims();
          if (!d.rw || !d.rh || !d.cw || !d.ch) return 1;
          return Math.min(1, (d.cw - FIT_PAD * 2) / d.rw, (d.ch - FIT_PAD * 2) / d.rh);
        }
        // Pannable only while the scaled component currently overflows the pane.
        function overflows() {
          var d = dims();
          return d.rw * scale > d.cw || d.rh * scale > d.ch;
        }
        function center() {
          var d = dims();
          tx = (d.cw - d.rw * scale) / 2;
          ty = (d.ch - d.rh * scale) / 2;
        }
        function apply() {
          stage.style.transform = "translate(" + tx + "px," + ty + "px) scale(" + scale + ")";
          canvas.classList.toggle("pannable", overflows());
        }

        // Initial / on content resize (until the user takes over): 100%,
        // centered. A larger-than-pane component thus starts cropped-but-centred
        // and can be zoomed out to reveal the whole of it.
        function reset() { scale = 1; center(); apply(); }
        var ro = new ResizeObserver(function () { if (!touched) reset(); });
        ro.observe(root);
        window.addEventListener("resize", function () { if (!touched) reset(); });

        // Wheel: zoom OUT only — never above 100%, only when the component is
        // larger than the pane, and anchored to the pane centre so it stays
        // centred (not toward the cursor).
        canvas.addEventListener("wheel", function (e) {
          if (!overflowsNatural()) return; // fits — let the page scroll
          e.preventDefault();
          var min = fitScale();
          var factor = Math.exp(-e.deltaY * 0.0015);
          var next = Math.min(1, Math.max(min, scale * factor));
          if (next === scale) return;
          touched = true;
          var d = dims(), ax = d.cw / 2, ay = d.ch / 2;
          tx = ax - (ax - tx) * (next / scale);
          ty = ay - (ay - ty) * (next / scale);
          scale = next;
          apply();
        }, { passive: false });

        // Drag: pan — only while the scaled content overflows the pane.
        var dragging = false, sx = 0, sy = 0;
        canvas.addEventListener("mousedown", function (e) {
          if (!overflows()) return;
          dragging = true; touched = true; sx = e.clientX - tx; sy = e.clientY - ty;
          canvas.classList.add("grabbing");
        });
        window.addEventListener("mousemove", function (e) {
          if (!dragging) return;
          tx = e.clientX - sx; ty = e.clientY - sy; apply();
        });
        window.addEventListener("mouseup", function () { dragging = false; canvas.classList.remove("grabbing"); });
      })();
    </script>
  </body>
</html>`;
}
