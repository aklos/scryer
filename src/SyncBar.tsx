import { useState } from "react";
import {
  Loader2,
  X,
  Check,
  Settings2,
  GitCompare,
  Flag,
  ChevronUp,
  ChevronDown,
} from "lucide-react";
import type { AgentSession } from "./hooks/useAgentSession";
import type { ModelBuild } from "./hooks/useModelBuild";
import type { DriftScope, ScryModel } from "./viewmodel";

interface SyncBarProps {
  model: ScryModel;
  agent: AgentSession;
  build: ModelBuild;
  projectPath: string | null;
  /** Boundary-owning nodes whose code changed since the last reconcile (a cheap,
   *  agent-free nudge). The bar shows the count and expands to list them. */
  driftScopes: DriftScope[];
  /** Jump the canvas to a node — used by the expanded drift panel's rows. */
  onRevealNode: (nodeId: string) => void;
  onOpenSettings: () => void;
}

export function SyncBar({
  model,
  agent,
  build,
  projectPath,
  driftScopes,
  onRevealNode,
  onOpenSettings,
}: SyncBarProps) {
  // The pre-check nudge expands into a panel listing exactly which nodes/files
  // changed since the last reconcile, so the count is never an opaque number.
  const [driftOpen, setDriftOpen] = useState(false);
  const changedScopes = driftScopes.length;

  // Drift review surface: behaviours the code has that the model didn't describe
  // (vagrant) and claims the code no longer discharges (changed).
  const resps = model.nodes.flatMap((n) => n.responsibilities ?? []);
  const vagrant = resps.filter((r) => r.vagrant).length;
  const changed = resps.filter((r) => r.status === "changed").length;
  const flagged = vagrant + changed;

  const runLabel = build.building
    ? "Building model"
    : build.checking
      ? "Checking for drift"
      : null;
  const busy = agent.running || build.active;

  return (
    <div className="shrink-0 border-t border-[var(--border)] bg-[var(--surface)] select-none">
      {busy && (
        <div className="h-0.5 w-full bg-[var(--border)] overflow-hidden">
          <div className="h-full w-1/3 bg-amber-500 dark:bg-amber-400 animate-[shimmer_1.5s_ease-in-out_infinite]" />
        </div>
      )}
      {/* Drift panel — expands upward from the powerline (like an editor's
          bottom panel) to show exactly which nodes and files drifted. */}
      {driftOpen && !busy && changedScopes > 0 && (
        <div className="flex flex-col max-h-64 border-b border-[var(--border)]">
          <div className="flex items-center justify-between h-7 px-3 shrink-0 border-b border-[var(--border)] text-[11px] text-[var(--text-tertiary)]">
            <span className="font-medium">
              {changedScopes} scope{changedScopes !== 1 ? "s" : ""} changed since
              last reconcile
            </span>
            <div className="flex items-center gap-1">
              <button
                type="button"
                disabled={!projectPath}
                onClick={() => {
                  if (!projectPath) return;
                  setDriftOpen(false);
                  build.checkDrift(projectPath);
                }}
                className="flex items-center gap-1 rounded px-2 py-0.5 font-medium text-amber-600 dark:text-amber-400 hover:bg-[var(--surface-hover)] cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-default"
                title="Run the semantic drift check over these scopes"
              >
                <GitCompare className="h-3 w-3" />
                Run drift check
              </button>
              <button
                type="button"
                onClick={() => setDriftOpen(false)}
                className="flex items-center rounded p-0.5 text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--surface-hover)] cursor-pointer transition-colors"
                aria-label="Close drift panel"
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          </div>
          <div className="overflow-auto py-1">
            {driftScopes.map((scope) => (
              <div key={scope.nodeId} className="px-2">
                <button
                  type="button"
                  onClick={() => {
                    setDriftOpen(false);
                    onRevealNode(scope.nodeId);
                  }}
                  className="group flex w-full items-center gap-2 rounded px-1.5 py-1 text-left hover:bg-[var(--surface-hover)] cursor-pointer transition-colors"
                  title={`Jump to ${scope.nodeName}`}
                >
                  <span className="text-[12px] font-medium text-[var(--text-secondary)] group-hover:text-[var(--text)] truncate">
                    {scope.nodeName}
                  </span>
                  <span className="shrink-0 text-[10px] text-[var(--text-muted)]">
                    {scope.changedFiles.length} file
                    {scope.changedFiles.length !== 1 ? "s" : ""}
                  </span>
                </button>
                <ul className="mb-1 mt-0.5 flex flex-col gap-px pl-3">
                  {scope.changedFiles.map((f) => (
                    <li
                      key={f}
                      className="truncate font-mono text-[10.5px] text-[var(--text-muted)]"
                      title={f}
                    >
                      {f}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>
      )}
      <div className="flex items-center h-7 px-3 gap-3 text-[11px]">
        {build.active ? (
          <>
            <div className="flex items-center gap-1.5 shrink-0">
              <div className="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse shrink-0" />
              <span className="text-[var(--text-tertiary)] font-medium">Agent</span>
            </div>
            <div className="w-px h-3 bg-[var(--border)]" />
            <div className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400 min-w-0">
              <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
              <span className="shrink-0">{build.phase ?? runLabel}</span>
              <span className="truncate text-[var(--text-muted)]">
                {build.activity ? `· ${build.activity}` : "…"}
              </span>
            </div>
            <div className="flex-1" />
            <button
              type="button"
              className="flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-medium text-[var(--text-tertiary)] hover:text-[var(--text)] hover:bg-[var(--surface-hover)] cursor-pointer transition-colors shrink-0"
              onClick={build.cancel}
            >
              <X className="h-3 w-3" />
              Cancel
            </button>
          </>
        ) : agent.running ? (
          <>
            <div className="flex items-center gap-1.5 shrink-0">
              <div className="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse shrink-0" />
              <span className="text-[var(--text-tertiary)] font-medium">Agent</span>
            </div>
            <div className="w-px h-3 bg-[var(--border)]" />
            <div className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400 min-w-0">
              <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
              <span className="shrink-0">{agent.label}</span>
              <span className="truncate text-[var(--text-muted)]">
                {agent.activity ? `· ${agent.activity}` : "…"}
              </span>
            </div>
            <div className="flex-1" />
            <button
              type="button"
              className="flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-medium text-[var(--text-tertiary)] hover:text-[var(--text)] hover:bg-[var(--surface-hover)] cursor-pointer transition-colors shrink-0"
              onClick={agent.cancel}
            >
              <X className="h-3 w-3" />
              Cancel
            </button>
          </>
        ) : (
          <>
            <div className="flex items-center gap-1.5 shrink-0">
              <div className="h-1.5 w-1.5 rounded-full bg-emerald-500 shrink-0" />
              <span className="text-[var(--text-tertiary)] font-medium">
                {model.nodes.length} node{model.nodes.length !== 1 ? "s" : ""}
              </span>
            </div>
            <div className="w-px h-3 bg-[var(--border)]" />
            <div className="flex items-center gap-1.5 text-[var(--text-muted)]">
              <Check className="h-3 w-3 text-emerald-500" />
              <span>
                {model.links.length} link{model.links.length !== 1 ? "s" : ""}
                {model.groups.length > 0 &&
                  ` · ${model.groups.length} group${model.groups.length !== 1 ? "s" : ""}`}
              </span>
            </div>
            {flagged > 0 && (
              <>
                <div className="w-px h-3 bg-[var(--border)]" />
                <div
                  className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400"
                  title={`${vagrant} undescribed behaviour(s), ${changed} stale claim(s) — review on the canvas`}
                >
                  <Flag className="h-3 w-3" />
                  <span>{flagged} to review</span>
                </div>
              </>
            )}
            <div className="flex-1" />
            <button
              type="button"
              disabled={!projectPath}
              onClick={() => {
                if (changedScopes > 0) setDriftOpen((o) => !o);
                else if (projectPath) build.checkDrift(projectPath);
              }}
              className={`flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-medium hover:bg-[var(--surface-hover)] cursor-pointer transition-colors shrink-0 disabled:opacity-40 disabled:cursor-default ${
                changedScopes > 0
                  ? "text-amber-600 dark:text-amber-400"
                  : "text-[var(--text-tertiary)] hover:text-[var(--text)]"
              } ${driftOpen ? "bg-[var(--surface-hover)]" : ""}`}
              title={
                changedScopes > 0
                  ? `${changedScopes} scope(s) changed since last reconcile — show what drifted`
                  : "Check the model against the code for semantic drift"
              }
            >
              <GitCompare className="h-3 w-3" />
              {changedScopes > 0 ? `Check drift (${changedScopes})` : "Check for drift"}
              {changedScopes > 0 &&
                (driftOpen ? (
                  <ChevronDown className="h-3 w-3" />
                ) : (
                  <ChevronUp className="h-3 w-3" />
                ))}
            </button>
          </>
        )}
        <button
          type="button"
          onClick={onOpenSettings}
          className="flex items-center rounded p-0.5 text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--surface-hover)] cursor-pointer transition-colors shrink-0"
          aria-label="Subagent settings"
          title="Subagent settings"
        >
          <Settings2 className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
