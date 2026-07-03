/**
 * Export-viewer entry.
 *
 * The model is baked into the bundle at build time: `export-viewer/vite.config`
 * reads the project's `.scryer/*.scry` and replaces `__SCRYER_MODEL_JSON__`
 * with its contents (as a string literal), so the shipped HTML carries the
 * whole model inline with no fetch and no backend. We parse it here and hand it
 * to `ExportApp`.
 */

import ReactDOM from "react-dom/client";
import "@fontsource-variable/space-grotesk";
import "../src/index.css";
import { applyTheme, DEFAULT_THEME } from "../src/theme";
import type { ScryModel } from "../src/viewmodel";
import { ExportApp } from "./ExportApp";

// Injected by the build (see export-viewer/vite.config.ts). Double-encoded: the
// define replaces this token with a JS string literal of the model's JSON text.
declare const __SCRYER_MODEL_JSON__: string;

const model = JSON.parse(__SCRYER_MODEL_JSON__) as ScryModel;

// A static page can't follow the OS theme the way the app does (no backend to
// listen to), so pin a concrete light theme for predictable colors anywhere.
applyTheme({ ...DEFAULT_THEME, colorMode: "light" });

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <ExportApp model={model} />,
);
