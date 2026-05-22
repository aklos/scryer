import { Loader2, X, Check } from "lucide-react";
import type { AgentSession } from "./hooks/useAgentSession";
import type { ScryModel } from "./viewmodel";

interface SyncBarProps {
  model: ScryModel;
  agent: AgentSession;
  projectPath: string | null;
}

export function SyncBar({ model, agent, projectPath }: SyncBarProps) {
  return (
    <div className="shrink-0 border-t border-[var(--border)] bg-[var(--surface)] select-none">
      {agent.running && (
        <div className="h-0.5 w-full bg-[var(--border)] overflow-hidden">
          <div className="h-full w-1/3 bg-amber-500 dark:bg-amber-400 animate-[shimmer_1.5s_ease-in-out_infinite]" />
        </div>
      )}
      <div className="flex items-center h-7 px-3 gap-3 text-[11px]">
        {/* Agent segment */}
        {agent.running ? (
          <>
            <div className="flex items-center gap-1.5 shrink-0">
              <div className="h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse shrink-0" />
              <span className="text-[var(--text-tertiary)] font-medium">Agent</span>
            </div>
            <div className="w-px h-3 bg-[var(--border)]" />
            <button
              type="button"
              className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400 cursor-pointer hover:text-amber-500 transition-colors"
              onClick={agent.cancel}
            >
              <Loader2 className="h-3 w-3 animate-spin" />
              <span>
                Filling {agent.label}
                {agent.lastTool ? ` · ${agent.lastTool}` : "…"}
              </span>
            </button>
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
            <div className="flex-1" />
          </>
        )}
        {projectPath && (
          <span className="text-[var(--text-muted)] shrink-0 truncate max-w-[300px]">
            {projectPath.replace(/^\/home\/[^/]+/, "~")}
          </span>
        )}
      </div>
    </div>
  );
}
