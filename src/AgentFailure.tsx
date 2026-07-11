/**
 * Agent-run failure modal.
 *
 * Any agent run (model build, drift check, per-node fill, visual variations) can
 * die mid-flight — most often an auth (401) or network error from the underlying
 * AI agent. A toast is too easy to miss for something that aborted real work, so
 * a failure surfaces here as a blocking modal that states two things plainly:
 * WHAT failed (the raw error from the agent) and WHAT SCRYER DID about it (the
 * recovery decision — e.g. "your drift state was left unchanged"), so the user
 * knows the model is in a known, safe state and what to do next.
 *
 * Mirrors the ToastProvider shape: a provider holds the one pending failure and
 * the `useAgentFailure()` consumer hook hands callers a `report` function.
 */

import { createContext, useCallback, useContext, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, X } from "lucide-react";
import { BTN, BTN_ICON } from "./pagekit";

export interface AgentFailure {
  /** Headline naming the run that failed, e.g. "Drift check failed". */
  title: string;
  /** The raw error from the agent (e.g. the 401 / network message). */
  error: string;
  /** What Scryer decided to do about it — the recovery/consequence, so the user
   *  knows the model's resulting state and the next step. */
  consequence: string;
}

interface AgentFailureContextValue {
  report: (failure: AgentFailure) => void;
}

const AgentFailureContext = createContext<AgentFailureContextValue>({ report: () => {} });

export const useAgentFailure = () => useContext(AgentFailureContext);

export function AgentFailureProvider({ children }: { children: ReactNode }) {
  const [failure, setFailure] = useState<AgentFailure | null>(null);

  const report = useCallback((f: AgentFailure) => setFailure(f), []);

  return (
    <AgentFailureContext.Provider value={{ report }}>
      {children}
      {failure && <AgentFailureModal failure={failure} onClose={() => setFailure(null)} />}
    </AgentFailureContext.Provider>
  );
}

function AgentFailureModal({
  failure,
  onClose,
}: {
  failure: AgentFailure;
  onClose: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/55 backdrop-blur-[3px]" onClick={onClose} />
      <div className="relative w-[460px] max-w-[90vw] rounded-lg border border-[var(--border-strong)] bg-[var(--surface)] shadow-2xl">
        <div className="flex items-center justify-between border-b border-[var(--border)] px-4 py-3">
          <span className="flex items-center gap-1.5 text-sm font-medium text-[var(--text)]">
            <AlertTriangle className="h-3.5 w-3.5 text-red-500 dark:text-red-400" />
            {failure.title}
          </span>
          <button
            type="button"
            onClick={onClose}
            className={BTN_ICON}
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="flex flex-col gap-3 px-4 py-4">
          {/* The raw error from the agent — wraps and scrolls so a long stack or
              provider message can't blow out the modal. */}
          <div className="max-h-[180px] overflow-auto rounded-md border border-[var(--border)] bg-[var(--surface-inset)] px-3 py-2">
            <p className="font-mono text-2xs leading-relaxed break-words whitespace-pre-wrap text-[var(--text-secondary)]">
              {failure.error}
            </p>
          </div>

          {/* What Scryer did about it — the recovery decision. */}
          <div className="flex flex-col gap-1">
            <span className="text-2xs font-medium uppercase tracking-[0.07em] text-[var(--text-tertiary)]">
              What Scryer did
            </span>
            <p className="text-xs leading-relaxed text-[var(--text-secondary)]">
              {failure.consequence}
            </p>
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" onClick={onClose} className={BTN}>
            Dismiss
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
