/**
 * Trailer harness entry.
 *
 * Default route autoplays the whole timeline (`#play`, or no hash). A scene id
 * hash (`#node`, `#drift`, …) renders that one beat in isolation for debugging;
 * `#loop` autoplays on repeat. `shoot.mjs` uses the single-scene hashes to grab
 * stills and `#play` to record the full trailer.
 */

import { useSyncExternalStore } from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/space-grotesk";
import "../src/index.css";
import { applyTheme, loadTheme } from "../src/theme";
import { sceneById } from "./scenes";
import { Runner } from "./runner";

// The trailer reads best on the dark canvas.
applyTheme({ ...loadTheme(), colorMode: "dark" });

const hash = () => location.hash.replace(/^#/, "");
function useHash(): string {
  return useSyncExternalStore(
    (cb) => {
      window.addEventListener("hashchange", cb);
      return () => window.removeEventListener("hashchange", cb);
    },
    hash,
  );
}

function DemoStage() {
  const h = useHash();
  if (!h || h === "play") return <Runner />;
  if (h === "loop") return <Runner loop />;
  return <>{sceneById(h).render()}</>;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<DemoStage />);
