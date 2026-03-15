import { useState, useEffect, useRef, useMemo } from "react";
import { RefreshCw, Loader2, Check, ChevronDown, ChevronUp, X, AlertCircle } from "lucide-react";

type DriftInfo = { nodeId: string; nodeName: string; patterns: string[] };

interface SyncBarProps {
  activeAgent: { name: string; available: boolean } | null;
  driftedNodes: DriftInfo[];
  structureChanged: boolean;
  syncStatus: "idle" | "running" | "error";
  syncMessage: string | null;
  onSync: () => void;
  onCancelSync: () => void;
  onDismissMessage: () => void;
  onNavigateToNode?: (nodeId: string) => void;
}

export function SyncBar({ activeAgent, driftedNodes, structureChanged, syncStatus, syncMessage, onSync, onCancelSync, onDismissMessage, onNavigateToNode }: SyncBarProps) {
  const [expanded, setExpanded] = useState(false);
  const sortedDriftedNodes = useMemo(
    () => [...driftedNodes].sort((a, b) => a.nodeName.localeCompare(b.nodeName)),
    [driftedNodes],
  );
  const hasDrift = driftedNodes.length > 0 || structureChanged;

  // Auto-dismiss success messages after 5s
  const isSuccess = syncStatus === "idle" && !!syncMessage && !hasDrift;
  const dismissRef = useRef(onDismissMessage);
  dismissRef.current = onDismissMessage;
  useEffect(() => {
    if (!isSuccess) return;
    const timer = setTimeout(() => dismissRef.current(), 5000);
    return () => clearTimeout(timer);
  }, [isSuccess]);

  // Don't render at all if no agent is available
  if (!activeAgent?.available) return null;

  const agentName = activeAgent.name;

  return (
    <div className="shrink-0 border-t border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-900 select-none">
      {/* Main bar row */}
      <div className="flex items-center h-7 px-3 gap-3 text-[11px]">
        {/* Agent identity */}
        <div className="flex items-center gap-1.5 shrink-0">
          <div className="h-1.5 w-1.5 rounded-full bg-emerald-500 shrink-0" />
          <span className="text-zinc-500 dark:text-zinc-400 font-medium">{agentName}</span>
        </div>

        <div className="w-px h-3 bg-zinc-200 dark:bg-zinc-700" />

        {/* Sync status */}
        {syncStatus === "running" ? (
          <>
            <div className="flex items-center gap-1.5 text-amber-600 dark:text-amber-400">
              <Loader2 className="h-3 w-3 animate-spin" />
              <span>Syncing…</span>
            </div>
            <button
              type="button"
              className="flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-medium text-zinc-500 hover:text-zinc-700 hover:bg-zinc-200 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-700 cursor-pointer transition-colors"
              onClick={onCancelSync}
            >
              <X className="h-3 w-3" />
              Cancel
            </button>
          </>
        ) : syncStatus === "error" && syncMessage ? (
          <>
            <div className="flex items-center gap-1.5 text-red-600 dark:text-red-400 min-w-0">
              <AlertCircle className="h-3 w-3 shrink-0" />
              <span className="truncate">{syncMessage}</span>
            </div>
            <button
              type="button"
              className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer transition-colors shrink-0"
              onClick={onDismissMessage}
              title="Dismiss"
            >
              <X className="h-3 w-3" />
            </button>
          </>
        ) : syncMessage && !hasDrift ? (
          // Completion summary — auto-dismisses after 5s
          <div className="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400 min-w-0">
            <Check className="h-3 w-3 shrink-0" />
            <span className="truncate">{syncMessage}</span>
          </div>
        ) : hasDrift ? (
          <>
            <div className="flex items-center gap-1.5 min-w-0">
              <div className="h-1.5 w-1.5 rounded-full bg-indigo-500 shrink-0" />
              <button
                type="button"
                className="text-zinc-600 dark:text-zinc-300 hover:text-zinc-800 dark:hover:text-zinc-100 cursor-pointer transition-colors flex items-center gap-1"
                onClick={() => driftedNodes.length > 0 && setExpanded((v) => !v)}
                title={driftedNodes.length > 0 ? "Show potentially drifted nodes" : undefined}
              >
                <span>
                  {driftedNodes.length > 0
                    ? `${driftedNodes.length} node${driftedNodes.length === 1 ? "" : "s"} may have drifted`
                    : "New files detected"}
                </span>
                {driftedNodes.length > 0 && (
                  expanded
                    ? <ChevronUp className="h-3 w-3 text-zinc-400" />
                    : <ChevronDown className="h-3 w-3 text-zinc-400" />
                )}
              </button>
            </div>

            {/* Inline drift summary (first few names) */}
            {!expanded && sortedDriftedNodes.length > 0 && (
              <div className="flex items-center gap-1 min-w-0 overflow-hidden">
                {sortedDriftedNodes.slice(0, 3).map((d) => (
                  <button
                    key={d.nodeId}
                    type="button"
                    className="rounded px-1.5 py-0.5 text-[10px] bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400 truncate max-w-[120px] cursor-pointer hover:bg-indigo-200 dark:hover:bg-indigo-800/30 transition-colors"
                    onClick={() => onNavigateToNode?.(d.nodeId)}
                  >
                    {d.nodeName}
                  </button>
                ))}
                {sortedDriftedNodes.length > 3 && (
                  <span className="text-[10px] text-zinc-400 dark:text-zinc-500 shrink-0">
                    +{sortedDriftedNodes.length - 3} more
                  </span>
                )}
              </div>
            )}

            <div className="flex-1" />

            {/* Sync action button */}
            <button
              type="button"
              className="flex items-center gap-1.5 rounded px-2.5 py-0.5 text-[11px] font-medium bg-emerald-500 text-white hover:bg-emerald-600 cursor-pointer transition-colors shrink-0"
              onClick={onSync}
            >
              <RefreshCw className="h-3 w-3" />
              Sync
            </button>
          </>
        ) : (
          <div className="flex items-center gap-1.5 text-zinc-400 dark:text-zinc-500">
            <Check className="h-3 w-3 text-emerald-500" />
            <span>In sync</span>
          </div>
        )}
      </div>

      {/* Expanded drift details */}
      {expanded && sortedDriftedNodes.length > 0 && syncStatus === "idle" && (
        <div className="border-t border-zinc-200/60 dark:border-zinc-700/60 px-3 py-1.5 space-y-1">
          {sortedDriftedNodes.map((d) => (
            <button
              key={d.nodeId}
              type="button"
              className="flex items-center gap-2 text-[11px] w-full text-left cursor-pointer hover:bg-zinc-100 dark:hover:bg-zinc-800 rounded px-1 -mx-1 transition-colors"
              onClick={() => onNavigateToNode?.(d.nodeId)}
            >
              <span className="text-zinc-600 dark:text-zinc-300 font-medium">{d.nodeName}</span>
              <span className="text-zinc-400 dark:text-zinc-500 truncate text-[10px]">
                {d.patterns.join(", ")}
              </span>
            </button>
          ))}
          {structureChanged && (
            <div className="flex items-center gap-2 text-[11px]">
              <span className="text-zinc-600 dark:text-zinc-300 font-medium">Project structure</span>
              <span className="text-zinc-400 dark:text-zinc-500 text-[10px]">New files detected</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
