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
import { Stage } from "./engine/Stage";
import { comprehendScene } from "./engine/scenes/comprehend";
import { prologueScene } from "./engine/scenes/prologue";
import { refundScene } from "./engine/scenes/refund";
import { ShellDemo } from "./engine/ShellDemo";
import { Terminal, type TerminalState } from "./engine/Terminal";

// A representative mid-stream state for previewing the terminal prop in
// isolation (`#terminal`): the request submitted, the orient + first two plan
// writes done, the last one still running (so the spinner shows). The real Act 1
// scene will type this in and stream the lines over time.
const TERMINAL_PREVIEW: TerminalState = {
  cwd: "~/aperture-pay",
  input: "",
  running: true,
  lines: [
    {
      kind: "user",
      text: "plan refund support — a cardholder should be able to get money back on a captured payment",
    },
    { kind: "tool", tool: "search_model", arg: "refund", status: "ok" },
    {
      kind: "say",
      text: "Refunds aren't modelled yet. I'll plan them across the services they touch — the ledger, the webhook dispatcher, and notifications. Writing it into the model now.",
    },
    { kind: "tool", tool: "update_nodes", target: "ledger", arg: "Post a refund as a reversing double-entry against the original capture", status: "ok" },
    { kind: "tool", tool: "update_nodes", target: "webhooks", arg: "Deliver refund.created and refund.settled to the merchant endpoint", status: "ok" },
    { kind: "tool", tool: "update_nodes", target: "notifications", arg: "Email a refund confirmation to the cardholder", status: "run" },
  ],
};

function TerminalPreview() {
  return (
    <div className="film-frame" style={{ display: "grid", placeItems: "center" }}>
      <div style={{ width: "min(760px, 92vw)", height: "min(560px, 88vh)" }}>
        <Terminal state={TERMINAL_PREVIEW} />
      </div>
    </div>
  );
}

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
  if (h === "comprehend") return <Stage scene={comprehendScene} />;
  if (h === "prologue") return <Stage scene={prologueScene} />;
  if (h === "refund") return <Stage scene={refundScene} />;
  if (h === "shell") return <ShellDemo />;
  if (h === "terminal") return <TerminalPreview />;
  return <>{sceneById(h).render()}</>;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<DemoStage />);
