/**
 * The powerline status bar — chevron-tiled segments along the window's bottom
 * edge (replacing the old SyncBar). Left side reads the model's standing at a
 * glance: the subagent launch setup (which agent + model + effort a fill runs
 * with), coverage, and the model's size + schema. Plan-ahead and drift counts
 * deliberately live on the tree's Changes/Drift lenses instead — they own both
 * the count and where it is. Right side folds in the live bits the SyncBar
 * carried — the agent's activity (barber-pole) with Cancel while it works, and
 * the review / recent-changes jumps when idle.
 *
 * Segment shades come from the theme surface vars via an inline `--sb`, which the
 * CSS reuses for each chevron's triangle so the tiling stays seamless (see the
 * `.pl` block in index.css).
 */

import type { CSSProperties } from "react";
import { History, Loader2, Sparkles, X } from "lucide-react";
import type { AgentSession } from "./hooks/useAgentSession";
import type { ModelBuild } from "./hooks/useModelBuild";
import type { ModelHealthReport } from "./health";
import type { ReviewIndex } from "./SpecialPages";
import { AGENT_LABEL, type ResolvedLaunch } from "./SettingsPanel";
import type { ScryModel } from "./viewmodel";
import type { SpecialPage } from "./NodePage";

interface PowerlineProps {
  model: ScryModel;
  agent: AgentSession;
  build: ModelBuild;
  /** Everything awaiting a human verdict (vagrant / stale / unmapped …). */
  reviewIndex: ReviewIndex;
  /** Coverage + flag totals; null until the first health fetch lands. */
  health: ModelHealthReport | null;
  /** The subagent launch setup a fill will run with — agent + model + effort. */
  launch: ResolvedLaunch;
  onOpenSpecial: (page: SpecialPage) => void;
  onOpenSettings: () => void;
}

/** Inline `--sb` carrier — drives both a segment's fill and its chevron edge. */
const sb = (value: string): CSSProperties => ({ ["--sb" as string]: value }) as CSSProperties;

export function Powerline({
  model,
  agent,
  build,
  reviewIndex,
  health,
  launch,
  onOpenSpecial,
  onOpenSettings,
}: PowerlineProps) {
  const totals = health?.health.totals;
  const coverage =
    totals && totals.anchorable > 0
      ? Math.round((totals.anchored / totals.anchorable) * 100)
      : null;

  const busy = agent.running || build.active;
  const runLabel = build.building
    ? "Building model"
    : build.checking
      ? "Checking for drift"
      : null;
  const agentLabel = build.active ? build.phase ?? runLabel : agent.label;
  const agentActivity = build.active ? build.activity : agent.activity;

  return (
    <div className={`pl ${busy ? "working" : ""}`}>
      {/* Agent launch — which subagent a fill will run with (model + effort);
          click to open the settings panel. Drift / plan-ahead used to live here,
          but the tree's Changes/Drift lenses already own those (count + where). */}
      <button
        type="button"
        onClick={onOpenSettings}
        className="seg cursor-pointer"
        style={sb("color-mix(in srgb, var(--text) 9%, var(--surface-canvas))")}
        title="Subagent launch setup — which agent fills the model, and its model + reasoning effort. Click to change."
      >
        <Sparkles className="h-3 w-3 shrink-0 text-violet-500 dark:text-violet-400" />
        {launch.agent ? (
          <>
            <span className="font-medium text-[var(--text-secondary)]">
              {AGENT_LABEL[launch.agent]}
            </span>
            <span className="text-[var(--text-tertiary)]">{launch.model || "default"}</span>
            <span className="text-[var(--text-ghost)]">·</span>
            <span className="text-[var(--text-tertiary)]">{launch.effort}</span>
          </>
        ) : (
          <span className="text-[var(--text-tertiary)]">no agent</span>
        )}
      </button>

      {/* Coverage — anchored / anchorable claims. */}
      <div className="seg" style={sb("color-mix(in srgb, var(--text) 4%, var(--surface-canvas))")}>
        {coverage != null ? (
          <>
            <span className="text-[var(--text-secondary)]">coverage</span>
            <span className="pl-strong font-medium">{coverage}%</span>
            <span className="text-[var(--text-tertiary)]">claims mapped</span>
          </>
        ) : (
          <span className="text-[var(--text-secondary)]">coverage —</span>
        )}
      </div>

      {/* Model size + schema. */}
      <div className="seg" style={sb("color-mix(in srgb, var(--text) 2.5%, var(--surface-canvas))")}>
        <span className="text-[var(--text-tertiary)]">
          {model.nodes.length} node{model.nodes.length === 1 ? "" : "s"} · {model.version}
        </span>
      </div>

      {/* Center: the live transient stream (tool calls, prose, status) while the
          agent works — plain, unstyled text so it stays clearly distinct from
          the violet "what it's doing" label on the right. Doubles as the flex
          spacer that pushes the right segments to the window edge when idle. */}
      <div className="pl-msg">
        {busy && agentActivity && (
          <span className="max-w-full truncate text-[var(--text-tertiary)]" title={agentActivity}>
            {agentActivity}
          </span>
        )}
      </div>

      {busy ? (
        <>
          <div
            className="rseg r-agent min-w-0"
            style={sb("color-mix(in srgb, var(--color-violet-500) 14%, var(--surface-active))")}
            title={agentLabel ?? undefined}
          >
            <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
            <span className="shrink-0">{agentLabel}</span>
          </div>
          <button
            type="button"
            onClick={build.active ? build.cancel : agent.cancel}
            className="rseg cursor-pointer"
            style={sb("color-mix(in srgb, var(--text) 9%, var(--surface-canvas))")}
          >
            <X className="h-3 w-3" />
            cancel
          </button>
        </>
      ) : (
        <>
          {reviewIndex.total > 0 && (
            <button
              type="button"
              onClick={() => onOpenSpecial("review")}
              className="rseg cursor-pointer"
              style={sb("color-mix(in srgb, var(--color-orange-500) 18%, var(--surface-canvas))")}
              title="Open Needs review — flags awaiting a human verdict (drift, unmapped claims, agent edits, empty symbols …)"
            >
              <span className="font-medium text-orange-600 dark:text-orange-400">
                {reviewIndex.total} flag{reviewIndex.total === 1 ? "" : "s"}
              </span>
            </button>
          )}
          <button
            type="button"
            onClick={() => onOpenSpecial("changes")}
            className="rseg cursor-pointer"
            style={sb("color-mix(in srgb, var(--text) 6.5%, var(--surface-canvas))")}
            title="Recent changes — the agent's edits this session"
          >
            <History className="h-3.5 w-3.5" />
            <span>changes</span>
          </button>
        </>
      )}
    </div>
  );
}
