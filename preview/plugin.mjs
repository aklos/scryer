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
 * @param {{ projectRoot: string, packageRoot?: string, useWrapper?: boolean }} opts
 *
 * `packageRoot` is the vite server's root — the project root itself in the
 * common case, or the sub-package that owns this server in a monorepo. All
 * `file`/`fixture` params and reported component paths stay PROJECT-relative
 * either way; only import URLs care about the package root: files inside it
 * import root-relative, files outside (the project-level .scryer/preview
 * fixtures and wrapper) import via /@fs/.
 */
export function scryerPreviewPlugin({ projectRoot, packageRoot = projectRoot, useWrapper = true }) {
  let analysis = null;
  const getAnalysis = () => (analysis ??= analyzeProject(projectRoot, packageRoot));

  /** Import URL for an absolute path, as seen from this vite server. */
  const toUrl = (abs) => {
    const rel = path.relative(packageRoot, abs);
    if (!rel.startsWith("..") && !path.isAbsolute(rel)) {
      return "/" + rel.split(path.sep).join("/");
    }
    const posix = abs.split(path.sep).join("/");
    return posix.startsWith("/") ? "/@fs" + posix : "/@fs/" + posix;
  };
  /** Import URL for a project-relative path. */
  const projUrl = (rel) => toUrl(path.join(projectRoot, rel));

  const wrapperFile = path.join(projectRoot, ".scryer", "preview", "Wrapper.tsx");
  const hasWrapper = () => useWrapper && fs.existsSync(wrapperFile);

  /** Global CSS the app entry imports — auto-applied to every preview. */
  const globalCss = detectGlobalCss(packageRoot);

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
      // synthesized defaults, when present. Project-relative, like `file`.
      const fixtureRel = params.get("fixture");
      const fixture =
        fixtureRel && !fixtureRel.includes("..") && fs.existsSync(path.join(projectRoot, fixtureRel))
          ? projUrl(fixtureRel)
          : null;

      const comp = getAnalysis().components.find(
        (c) => c.file === file && c.exportName === exportName,
      );
      const cssImports = globalCss.map((c) => `import ${JSON.stringify(c)};`).join("\n");
      // Framework dispatch: the component list tagged the file with what mounts
      // it. Both entries share the render-verdict and fixture contract.
      if (comp?.framework === "vue" || file.endsWith(".vue")) {
        return vueEntryModule({ file, fixture, cssImports, importUrl: projUrl(file) });
      }
      const propsCode = comp?.propsCode ?? "{}";
      // Shared, type-keyed fixtures (B6) referenced by the synthesized props —
      // imported under the tokens props.mjs baked into propsCode.
      const refImports = (comp?.fixtureRefs ?? [])
        .map((r) => `import { ${r.export} as ${r.token} } from ${JSON.stringify(projUrl(".scryer/preview/fixtures/" + r.module))};`)
        .join("\n");
      const wrapper = hasWrapper();

      return `
${wrapper ? `import Wrapper from ${JSON.stringify(toUrl(wrapperFile))};` : ""}
${fixture ? `import __fixture from ${JSON.stringify(fixture)};` : ""}
${refImports}
import * as React from "react";
import { createRoot } from "react-dom/client";
${cssImports}
import * as Mod from ${JSON.stringify(projUrl(file))};

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
      // A sub-package vite server only watches its own root by default — the
      // project-level .scryer/preview dir must be added explicitly.
      if (packageRoot !== projectRoot) {
        server.watcher.add(path.join(projectRoot, ".scryer", "preview"));
      }
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

        // The app's webview is always a different origin from this server (the
        // `tauri://localhost` custom scheme in a bundled app, `localhost:1420`
        // in dev), so its `/__components.json` fetch is cross-origin. This
        // middleware is registered from `configureServer`, whose body runs
        // BEFORE vite installs its own cors middleware — our routes answer
        // first and nothing downstream ever adds the header. Vite's cors
        // default wouldn't cover `tauri://` anyway (since CVE-2025-24010 it
        // only allows http(s) localhost origins). Without this, WebKit fails
        // the fetch as a bare "TypeError: Load failed" and the Preview section
        // reports a server that is in fact running fine. The multi-package
        // router in server.mjs sets the same header on its merged list.
        if (url.pathname === "/__preview" || url.pathname === "/__components.json") {
          res.setHeader("Access-Control-Allow-Origin", "*");
          res.setHeader("Vary", "Origin");
        }

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

/** CSS files imported by the app entry (main.tsx etc.), as root-relative paths.
 *  Also catches bound/query imports like `import styles from "./x.css?inline"`
 *  (a shadow-root widget pattern) — the query is dropped so the preview gets a
 *  plain document-level stylesheet. */
function detectGlobalCss(projectRoot) {
  for (const entry of ["src/main.tsx", "src/main.jsx", "src/index.tsx", "src/main.ts"]) {
    const full = path.join(projectRoot, entry);
    if (!fs.existsSync(full)) continue;
    const source = fs.readFileSync(full, "utf8");
    const css = [];
    for (const m of source.matchAll(/import\s+(?:[\w$]+\s+from\s+)?["']([^"']+\.css)(?:\?[^"']*)?["']/g)) {
      const resolved = m[1].startsWith(".")
        ? "/" + path.posix.join(path.posix.dirname(entry), m[1])
        : m[1]; // bare package import — leave for Vite to resolve
      css.push(resolved);
    }
    return css;
  }
  return [];
}

/**
 * The entry module for a Vue single-file component. Mounts it with the
 * project's own `vue` (`createApp`), synthesizing placeholder props AT MOUNT
 * TIME from the compiled component's normalized `props` definition — which
 * Vue produces for `defineProps<{…}>()` and runtime prop objects alike, so no
 * static analysis is needed — and filling its default slot so a wrapper isn't
 * blank. Reports the same `scryer-render` verdict as the React entry; a
 * per-node fixture (a `.ts` module default-exporting props) spreads over the
 * synthesized ones exactly as for React. `Wrapper.tsx` is React-only and is
 * not applied here.
 */
function vueEntryModule({ file, fixture, cssImports, importUrl }) {
  return `
${fixture ? `import __fixture from ${JSON.stringify(fixture)};` : ""}
${cssImports}
import { createApp, h } from "vue";
import Component from ${JSON.stringify(importUrl)};

const meta = { file: ${JSON.stringify(file)}, exportName: "default" };
const __hasFixture = ${fixture ? "true" : "false"};
function report(status, error) {
  try { parent.postMessage({ type: "scryer-render", ...meta, status, error: error ?? null, hasFixture: __hasFixture }, "*"); } catch {}
}
const bodyBaseline = document.body.childElementCount;
function scheduleOkCheck() {
  requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(() => {
    const root = document.getElementById("root");
    const rendered = (root && root.childNodes.length > 0) || document.body.childElementCount > bodyBaseline;
    report(rendered ? "ok" : "empty");
  }, 120)));
}

// Placeholder props from Vue's normalized props definition: only REQUIRED
// props are filled (optionals keep their defaults, as with React), typed by
// the first declared constructor.
function pretty(name) { return "Sample " + name.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase(); }
function placeholder(name, ctor) {
  if (ctor === String) return /(url|href|src)$/i.test(name) ? "https://example.com" : pretty(name);
  if (ctor === Number) return 1;
  if (ctor === Boolean) return false;
  if (ctor === Array) return [];
  if (ctor === Function) return () => {};
  if (ctor === Date) return new Date(0);
  if (ctor === Object) return {};
  return pretty(name);
}
function synthVueProps(def) {
  const out = {};
  if (!def) return out;
  const entries = Array.isArray(def) ? def.map((k) => [k, null]) : Object.entries(def);
  for (const [name, opt] of entries) {
    const o = opt && typeof opt === "object" && !Array.isArray(opt) ? opt : { type: opt };
    if (!o.required || o.default !== undefined) continue;
    const ctor = Array.isArray(o.type) ? o.type[0] : o.type;
    out[name] = placeholder(name, ctor);
  }
  return out;
}

window.addEventListener("error", (e) => report("error", String(e.error?.stack ?? e.message)));
window.addEventListener("unhandledrejection", (e) => report("error", "unhandled rejection: " + String(e.reason)));

if (!Component) {
  report("error", "no default export in " + meta.file);
} else {
  try {
    const props = ${fixture ? "{ ...synthVueProps(Component.props), ...__fixture }" : "synthVueProps(Component.props)"};
    const app = createApp({ render: () => h(Component, props, { default: () => "Sample children" }) });
    app.config.errorHandler = (err) => report("error", String(err?.stack ?? err));
    app.mount(document.getElementById("root"));
    scheduleOkCheck();
  } catch (err) {
    report("error", String(err?.stack ?? err));
  }
}
`;
}

// `componentDark` themes the previewed component (best-effort: a .dark class +
// color-scheme on the iframe — works for class-based theming like Tailwind, a
// no-op for prefers-color-scheme / provider-based theming we can't reach from an
// isolated iframe). `canvasDark` is scryer's chrome theme and drives ONLY the
// checkerboard backdrop, so the two stay decoupled.
function previewHtml(entrySrc, componentDark = false, canvasDark = false) {
  const fgFallback = componentDark ? "#f5f5f5" : "#111111";
  // The component mounts into #root (entry contract unchanged); #root sits on
  // the transformed stage (<body>) inside the clipped viewport (<html>). The
  // viewport paints a low-contrast transparency checkerboard so the component
  // reads as clearly separate from it. A small harness centers the component at
  // 100%; when it is larger than the pane you can zoom (cursor-anchored,
  // between fit and 100%) and pan (clamped so content keeps covering the pane —
  // no free-floating). Viewport-pinned components (position:fixed widgets) get
  // viewport mode: the stage becomes a virtual 1280×800 screen dressed as a faux
  // host page (browser bar + greeked content) that the component anchors to,
  // starting focused on the component itself with the page a zoom-out away.
  // Two near-equal greys per theme — subtle texture, not a loud checker.
  const checkA = canvasDark ? "#202020" : "#dcdcdc";
  const checkB = canvasDark ? "#181818" : "#f1f1f1";
  return `<!doctype html>
<html${componentDark ? ` class="dark"` : ""}>
  <head>
    <meta charset="utf-8" />
    <style>
      /* <html> is the clipped, checkerboard VIEWPORT and <body> the transformed
         STAGE — not a nested div, so that a component which portals into
         document.body (modals, toasts, dropdown menus) still lands INSIDE the
         stage and the same fit / zoom / pan applies to it. */
      html {
        position: fixed; inset: 0; margin: 0; overflow: hidden;
        color-scheme: ${componentDark ? "dark" : "light"};
        background-color: ${checkB};
        background-image: repeating-conic-gradient(${checkA} 0% 25%, ${checkB} 0% 50%);
        background-size: 16px 16px;
        cursor: default;
      }
      html.pannable { cursor: grab; }
      html.grabbing { cursor: grabbing; }
      html.grabbing, html.grabbing * { user-select: none; -webkit-user-select: none; }
      body {
        position: absolute; top: 0; left: 0; margin: 0;
        transform-origin: 0 0; will-change: transform;
        color: var(--text, ${fgFallback});
      }
      /* Viewport mode: the stage becomes a virtual screen the pinned component
         anchors to — dressed as a faux host page (browser bar + greeked
         content) so the backdrop reads as context, never as the component. */
      body.viewport {
        background: ${componentDark ? "#101216" : "#ffffff"};
        outline: 1px solid ${canvasDark ? "rgba(255,255,255,0.14)" : "rgba(17,17,17,0.18)"};
      }
      #page { position: absolute; inset: 0; display: none; pointer-events: none; }
      body.viewport #page { display: flex; flex-direction: column; }
      #page .bar {
        height: 44px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 16px;
        background: ${componentDark ? "#161a20" : "#f3f4f6"};
        border-bottom: 1px solid ${componentDark ? "#242a33" : "#e5e7eb"};
      }
      #page .dot { width: 10px; height: 10px; border-radius: 50%; background: ${componentDark ? "#2e3540" : "#d6d9df"}; }
      #page .url {
        width: 320px; height: 20px; margin-left: 12px; border-radius: 10px;
        background: ${componentDark ? "#20252d" : "#e9ebef"};
      }
      #page .body { flex: 1; padding: 48px 64px; }
      #page .blk { border-radius: 8px; background: ${componentDark ? "#181d24" : "#eef0f3"}; margin-bottom: 20px; }
      #root { display: inline-block; }
    </style>
  </head>
  <body>
    <div id="page" aria-hidden="true">
      <div class="bar"><span class="dot"></span><span class="dot"></span><span class="dot"></span><span class="url"></span></div>
      <div class="body">
        <div class="blk" style="height:36px;width:42%"></div>
        <div class="blk" style="height:14px;width:92%"></div>
        <div class="blk" style="height:14px;width:78%"></div>
        <div class="blk" style="height:14px;width:85%"></div>
        <div class="blk" style="height:220px;width:60%;margin-top:32px"></div>
        <div class="blk" style="height:14px;width:88%;margin-top:32px"></div>
        <div class="blk" style="height:14px;width:70%"></div>
      </div>
    </div><div id="root"></div>
    <script type="module" src="${entrySrc}"></script>
    <script>
      (function () {
        var canvas = document.documentElement;
        var stage = document.body;
        var root = document.getElementById("root");
        var page = document.getElementById("page");
        // The elements the component put on stage: what it mounted into #root,
        // plus anything it portalled straight onto the body.
        function stagedKids() {
          var out = [], i, k;
          for (i = 0; i < root.children.length; i++) out.push(root.children[i]);
          for (i = 0; i < stage.children.length; i++) {
            k = stage.children[i];
            if (k === root || k === page || k.tagName === "SCRIPT" || k.tagName === "STYLE") continue;
            out.push(k);
          }
          return out;
        }
        var FIT_PAD = 24;
        var scale = 1, tx = 0, ty = 0, touched = false;

        // Viewport mode — for components pinned to the viewport (position:
        // fixed widgets, overlays). Their #root measures 0×0, and the
        // transformed stage is their containing block, so they'd anchor to a
        // zero-sized box. Instead the containing-block quirk becomes the
        // feature: the stage gets an explicit virtual screen, the component pins
        // to ITS corners, and the usual fit/zoom/pan math runs against that
        // rectangle — scaled to fit initially.
        var VW = 1280, VH = 800;
        var viewportMode = false;

        // The content's painted extent in stage space (origin may be negative:
        // a fixed-inset modal taller than the virtual screen spills above it).
        // In viewport mode that is the virtual screen unioned with everything
        // painted; otherwise #root's box. Re-measured on every reset, cached
        // between — pan/zoom math runs per frame and must not walk the DOM.
        var extent = { x: 0, y: 0, w: 0, h: 0 };
        function measureExtent() {
          if (!viewportMode) {
            extent = { x: 0, y: 0, w: root.scrollWidth, h: root.scrollHeight };
            return;
          }
          var bb = paintedBBox();
          var x0 = 0, y0 = 0, x1 = VW, y1 = VH;
          if (bb) {
            x0 = Math.min(x0, bb.x); y0 = Math.min(y0, bb.y);
            x1 = Math.max(x1, bb.x + bb.w); y1 = Math.max(y1, bb.y + bb.h);
          }
          extent = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
        }
        function dims() {
          return { cw: canvas.clientWidth, ch: canvas.clientHeight, rw: extent.w, rh: extent.h };
        }

        function detectViewportMode() {
          if (viewportMode) return;
          var kids = stagedKids(), pinned = false, i;
          for (i = 0; i < kids.length && !pinned; i++) {
            if (getComputedStyle(kids[i]).position === "fixed") pinned = true;
          }
          // Fixed content deeper down: a zero-size root that still paints.
          if (!pinned && (root.scrollWidth < 8 || root.scrollHeight < 8)) {
            for (i = 0; i < kids.length && !pinned; i++) {
              var b = kids[i].getBoundingClientRect();
              if (b.width > 8 && b.height > 8) pinned = true;
            }
          }
          if (!pinned) return;
          viewportMode = true;
          stage.classList.add("viewport");
          stage.style.width = VW + "px";
          stage.style.height = VH + "px";
          if (!touched) reset();
        }
        // Until viewport mode is known, each mutation re-runs detection; after
        // that, mutations re-measure the painted extent (a modal that grows
        // after mount) — #root stays 0×0 there, so the ResizeObserver is blind.
        var detectQueued = false;
        new MutationObserver(function () {
          if (detectQueued) return;
          detectQueued = true;
          requestAnimationFrame(function () {
            detectQueued = false;
            if (!viewportMode) detectViewportMode();
            else if (!touched) reset();
            else { measureExtent(); clampPan(); apply(); }
          });
        }).observe(stage, { childList: true, subtree: true });
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
          tx = (d.cw - d.rw * scale) / 2 - extent.x * scale;
          ty = (d.ch - d.rh * scale) / 2 - extent.y * scale;
        }
        function apply() {
          stage.style.transform = "translate(" + tx + "px," + ty + "px) scale(" + scale + ")";
          canvas.classList.toggle("pannable", overflows());
        }

        // Keep the content reachable, never lost: on an axis where the painted
        // extent is larger than the pane it may not pull an edge inside the
        // pane (no void beside it); where it is smaller it may move freely but
        // may not leave the pane. Measured on the EXTENT, so a modal that
        // spills past the virtual screen can still be panned up to its top.
        function clampAxis(pos, size, pane) {
          return size >= pane ? Math.min(0, Math.max(pane - size, pos)) : Math.min(pane - size, Math.max(0, pos));
        }
        function clampPan() {
          var d = dims(), w = d.rw * scale, h = d.rh * scale;
          // Pane-space position of the extent's top-left corner.
          var lx = tx + extent.x * scale, ly = ty + extent.y * scale;
          tx = clampAxis(lx, w, d.cw) - extent.x * scale;
          ty = clampAxis(ly, h, d.ch) - extent.y * scale;
        }

        // Stage-space union of a set of boxes (client rects → stage coords).
        function unionBBox(els) {
          var sr = stage.getBoundingClientRect();
          var x0 = 1 / 0, y0 = 1 / 0, x1 = -1 / 0, y1 = -1 / 0, found = false;
          for (var i = 0; i < els.length; i++) {
            var b = els[i].getBoundingClientRect();
            if (b.width < 2 || b.height < 2) continue;
            found = true;
            x0 = Math.min(x0, (b.left - sr.left) / scale);
            y0 = Math.min(y0, (b.top - sr.top) / scale);
            x1 = Math.max(x1, (b.right - sr.left) / scale);
            y1 = Math.max(y1, (b.bottom - sr.top) / scale);
          }
          return found ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null;
        }
        // The staged elements themselves — in viewport mode the pinned widget
        // (fixed children don't grow #root, so they're measured individually).
        function contentBBox() { return unionBBox(stagedKids()); }
        // Everything painted: the staged elements AND their descendants. A
        // fixed/inset-0 container reports the screen box while its
        // centred child spills past both ends — only the descendants show that.
        var PAINT_CAP = 4000;
        function paintedBBox() {
          var kids = stagedKids(), els = kids.slice();
          for (var i = 0; i < kids.length && els.length < PAINT_CAP; i++) {
            var all = kids[i].querySelectorAll("*");
            for (var j = 0; j < all.length && els.length < PAINT_CAP; j++) els.push(all[j]);
          }
          return unionBBox(els);
        }

        // Initial / on content resize (until the user takes over). Normal
        // components: 100%, centred — larger-than-pane ones start
        // cropped-but-centred and zoom out to reveal the rest. Viewport mode:
        // focus the widget itself — whole widget visible, as close to 100% as
        // that allows — with the host page around it one zoom-out away.
        function reset() {
          measureExtent();
          if (viewportMode) {
            var d = dims(), bb = contentBBox();
            if (bb && bb.w > 4 && bb.h > 4) {
              scale = Math.max(fitScale(), Math.min(1, (d.cw - FIT_PAD * 2) / bb.w, (d.ch - FIT_PAD * 2) / bb.h));
              tx = d.cw / 2 - (bb.x + bb.w / 2) * scale;
              ty = d.ch / 2 - (bb.y + bb.h / 2) * scale;
              clampPan();
            } else {
              scale = fitScale();
              center();
            }
          } else {
            scale = 1;
            center();
          }
          clampPan();
          apply();
        }
        var ro = new ResizeObserver(function () { if (!touched) reset(); });
        ro.observe(root);
        window.addEventListener("resize", function () { if (!touched) reset(); });

        // Wheel: cursor-anchored zoom between fit and 100% — the view
        // converges on what you point at, so refocusing after a zoom-out is
        // point-and-wheel, no panning needed. Clamped so zooming never opens a
        // gap at the edges.
        canvas.addEventListener("wheel", function (e) {
          if (!overflowsNatural()) return; // fits at 100% — let the page scroll
          e.preventDefault();
          var min = fitScale();
          var factor = Math.exp(-e.deltaY * 0.0015);
          var next = Math.min(1, Math.max(min, scale * factor));
          if (next === scale) return;
          touched = true;
          tx = e.clientX - (e.clientX - tx) * (next / scale);
          ty = e.clientY - (e.clientY - ty) * (next / scale);
          scale = next;
          clampPan();
          apply();
        }, { passive: false });

        // Drag: pan — only while the scaled content overflows the pane.
        var dragging = false, sx = 0, sy = 0;
        canvas.addEventListener("mousedown", function (e) {
          if (e.button !== 0 || !overflows()) return;
          e.preventDefault(); // no text selection / native drag while panning
          dragging = true; touched = true; sx = e.clientX - tx; sy = e.clientY - ty;
          canvas.classList.add("grabbing");
        });
        window.addEventListener("mousemove", function (e) {
          if (!dragging) return;
          tx = e.clientX - sx; ty = e.clientY - sy; clampPan(); apply();
        });
        window.addEventListener("mouseup", function () { dragging = false; canvas.classList.remove("grabbing"); });
      })();
    </script>
  </body>
</html>`;
}
