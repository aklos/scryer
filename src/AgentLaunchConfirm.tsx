/**
 * Confirm gate shown before any UI action that launches an agent (a billable,
 * possibly long-running fill). It names exactly what will run — the resolved
 * agent + model + effort, the same readout the powerline carries — so the user
 * is never surprised by what a violet button does, which matters most when the
 * configured model is expensive or their quota is thin. "Don't ask again" clears
 * the gate (persisted via the subagent settings); the violet button styling then
 * remains the standing cue that the action still spawns an agent.
 */

import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { AGENT_LABEL, type ResolvedLaunch } from "./SettingsPanel";
import type { LaunchSettings } from "./hooks/useLaunchSettings";
import { AgentMark, BTN, BTN_AGENT, BTN_ICON } from "./pagekit";

export interface AgentLaunchRequest {
  /** Short imperative naming the action, e.g. "Rebuild the model from your codebase". */
  action: string;
  /** Optional second line — scope or cost specifics for this particular action. */
  detail?: string;
}

export interface AgentLaunchGate {
  /** Gate an agent launch: runs `launch` immediately if confirmation is off,
   *  otherwise opens the modal and runs it only on "Run agent". */
  request: (request: AgentLaunchRequest, launch: () => void) => void;
  /** The confirm modal to render (null when nothing is pending). */
  modal: React.ReactNode;
}

/** Wraps the four agent-spawning UI actions in the confirm gate. Holds the one
 *  pending request and renders the modal for it; "don't ask again" clears the
 *  setting so future launches fire straight through. */
export function useAgentLaunchGate(
  settings: LaunchSettings,
  /** Opens the subagent settings — the dialog offers it beside the resolved
   *  launch so the model and effort can be changed before a costly run. */
  onOpenSettings?: () => void,
): AgentLaunchGate {
  const { launch, confirmLaunch, clearConfirm } = settings;
  const [pending, setPending] = useState<{ request: AgentLaunchRequest; run: () => void } | null>(
    null,
  );

  const request = useCallback(
    (req: AgentLaunchRequest, run: () => void) => {
      if (!confirmLaunch) {
        run();
        return;
      }
      setPending({ request: req, run });
    },
    [confirmLaunch],
  );

  const modal = pending ? (
    <AgentLaunchConfirm
      launch={launch}
      request={pending.request}
      onOpenSettings={onOpenSettings}
      onCancel={() => setPending(null)}
      onConfirm={(dontAskAgain) => {
        if (dontAskAgain) void clearConfirm();
        pending.run();
        setPending(null);
      }}
    />
  ) : null;

  return { request, modal };
}

export function AgentLaunchConfirm({
  launch,
  request,
  onConfirm,
  onCancel,
  onOpenSettings,
}: {
  launch: ResolvedLaunch;
  request: AgentLaunchRequest;
  /** When given, the dialog offers a "Change…" beside the resolved launch. */
  onOpenSettings?: () => void;
  /** Proceed. `dontAskAgain` true means the user wants the gate cleared. */
  onConfirm: (dontAskAgain: boolean) => void;
  onCancel: () => void;
}) {
  const [dontAsk, setDontAsk] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/55 backdrop-blur-[3px]" onClick={onCancel} />
      <div className="relative w-[420px] max-w-[90vw] rounded-lg border border-[var(--border-strong)] bg-[var(--surface)] shadow-2xl">
        <div className="flex items-center justify-between border-b border-[var(--border)] px-4 py-3">
          <span className="flex items-center gap-1.5 text-sm font-medium text-[var(--text)]">
            <AgentMark />
            Run an agent?
          </span>
          <button
            type="button"
            onClick={onCancel}
            className={BTN_ICON}
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="flex flex-col gap-3 px-4 py-4">
          <p className="text-sm text-[var(--text)]">{request.action}</p>

          {/* The resolved launch — mirrors the powerline so the two never disagree. */}
          <div className="flex items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-inset)] px-3 py-2 text-xs">
            <AgentMark />
            {launch.agent ? (
              <span className="flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
                <span className="font-medium text-[var(--text)]">{AGENT_LABEL[launch.agent]}</span>
                <span className="text-[var(--text-tertiary)]">{launch.model || "default model"}</span>
                <span className="text-[var(--text-ghost)]">·</span>
                <span className="text-[var(--text-tertiary)]">{launch.effort} effort</span>
              </span>
            ) : (
              <span className="text-[var(--text-tertiary)]">
                No agent detected — install Claude Code, Codex or Copilot CLI.
              </span>
            )}
          </div>

          <p className="text-xs leading-relaxed text-[var(--text-muted)]">
            {request.detail ? `${request.detail} ` : ""}
            This uses your configured AI agent and can take a while and use significant tokens —
            mind the model you&rsquo;ve set if you pay per token.{" "}
            {onOpenSettings ? (
              <>
                You can{" "}
                <button
                  type="button"
                  className="underline decoration-dotted underline-offset-2 hover:text-[var(--text)]"
                  onClick={() => {
                    onCancel();
                    onOpenSettings();
                  }}
                >
                  change the agent, model or effort
                </button>{" "}
                first, and cancel any run mid-flight.
              </>
            ) : (
              <>You can change it in subagent settings, and cancel any run mid-flight.</>
            )}
          </p>

          <label className="flex cursor-pointer items-center gap-2 text-xs text-[var(--text-secondary)] select-none">
            <input
              type="checkbox"
              checked={dontAsk}
              onChange={(e) => setDontAsk(e.target.checked)}
              className="accent-violet-500"
            />
            Don&rsquo;t ask again
          </label>
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" onClick={onCancel} className={BTN}>
            Cancel
          </button>
          <button
            type="button"
            onClick={() => onConfirm(dontAsk)}
            disabled={!launch.agent}
            className={`${BTN_AGENT} disabled:cursor-default disabled:opacity-50`}
          >
            <AgentMark className="" />
            Run agent
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
