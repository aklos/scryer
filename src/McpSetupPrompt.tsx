/**
 * The "Enable AI tool integration" prompt — restored after the canvas rebuild
 * dropped it. Offers one-click MCP setup for the detected agent(s) and names the
 * exact files it will create, so nothing is written into the user's project
 * without consent. Presentational only: it owns the card, not its placement —
 * the canvas mounts it as a dismissible overlay, the new-project screen inline.
 */

import { X } from "lucide-react";
import type { McpSetup } from "./hooks/useMcpSetup";
import { BTN, BTN_GO } from "./pagekit";

/** The files `enable()` will create or merge into, given what's detected and
 *  still missing — the consent list shown to the user. */
function plannedWrites(tools: McpSetup["tools"]): string[] {
  const out: string[] = [];
  // Claude Code and Copilot share `.mcp.json`, so it's listed once for either.
  if ((tools.claude && !tools.claudeMcpEnabled) || (tools.copilot && !tools.copilotMcpEnabled))
    out.push(".mcp.json");
  if (tools.codex && !tools.codexMcpEnabled) out.push(".codex/config.toml");
  if (tools.claude && !tools.claudeApproved)
    out.push(".claude/settings.local.json — auto-approve all scryer tools");
  return out;
}

export function McpSetupPrompt({
  setup,
  onDone,
  dismissable,
}: {
  setup: McpSetup;
  /** Called after a successful enable — lets the host refresh dependent state
   *  (e.g. the launch readout) now that an agent can reach the model. */
  onDone?: () => void;
  /** Show a corner ✕ (the floating canvas variant). Inline hosts leave it off
   *  and rely on the "Not now" button. Both routes call `setup.dismiss()`. */
  dismissable?: boolean;
}) {
  const { claude, codex, copilot } = setup.tools;
  const names = [claude && "Claude Code", codex && "Codex", copilot && "Copilot CLI"].filter(
    Boolean,
  ) as string[];
  // "A", "A and B", "A, B and C" — three agents can be detected at once.
  const agents =
    names.length > 2
      ? `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`
      : names.join(" and ");
  const writes = plannedWrites(setup.tools);

  const enable = async () => {
    await setup.enable();
    onDone?.();
  };

  return (
    <div className="relative flex flex-col gap-2 rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] px-4 py-3 shadow-lg backdrop-blur-sm">
      {dismissable && (
        <button
          type="button"
          onClick={setup.dismiss}
          className="absolute right-2 top-2 text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
          aria-label="Dismiss"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      )}
      <div className="pr-4 text-xs font-medium text-[var(--text)]">Enable AI tool integration</div>
      <div className="text-xs leading-relaxed text-[var(--text-muted)]">
        {agents} {names.length > 1 ? "are" : "is"} installed. Wire scryer into this project so your
        agent can read and update the model over MCP. This creates:
      </div>
      <ul className="flex flex-col gap-0.5 text-xs text-[var(--text-secondary)]">
        {writes.map((w) => (
          <li key={w} className="font-mono text-xs">
            {w}
          </li>
        ))}
      </ul>
      <div className="mt-0.5 flex items-center gap-2">
        <button type="button" className={BTN_GO} disabled={setup.busy} onClick={enable}>
          {setup.busy ? "Enabling…" : "Enable"}
        </button>
        <button type="button" className={BTN} disabled={setup.busy} onClick={setup.dismiss}>
          Not now
        </button>
      </div>
    </div>
  );
}
