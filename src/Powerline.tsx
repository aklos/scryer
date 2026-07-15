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
import { Flag, History, Loader2, X } from "lucide-react";
import { AgentMark } from "./pagekit";
import type { AgentSession } from "./hooks/useAgentSession";
import type { ModelBuild } from "./hooks/useModelBuild";
import { darkBoundaries, type ModelHealthReport } from "./health";
import type { ReviewIndex } from "./SpecialPages";
import { AGENT_LABEL, type ResolvedLaunch } from "./SettingsPanel";
import type { ScryModel } from "./viewmodel";
import type { SpecialPage } from "./NodePage";

interface PowerlineProps {
  model: ScryModel;
  agent: AgentSession;
  build: ModelBuild;
  /** Everything awaiting a human verdict (vagrant / stale / agent edits …). */
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
  // The other failure mode: code under a boundary that no claim reads into.
  const dark = darkBoundaries(health).total;

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
        className="seg"
        style={sb("color-mix(in srgb, var(--text) 9%, var(--surface-canvas))")}
        title="Subagent launch setup — which agent fills the model, and its model + reasoning effort. Click to change."
      >
        <AgentMark />
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

      {/* Coverage — anchored / anchorable claims (the model→code direction).
          Click to list the unmapped claims (the inverse; symmetric with the
          dark-code segment that follows). */}
      <button
        type="button"
        onClick={() => onOpenSpecial("unmapped")}
        className="seg"
        style={sb("color-mix(in srgb, var(--text) 4%, var(--surface-canvas))")}
        title="Claims mapped — committed claims that read through to code. Click to list the ones that map to nothing."
      >
        {coverage != null ? (
          <>
            <span className="pl-strong font-medium">{coverage}%</span>
            <span className="text-[var(--text-secondary)]">claims mapped</span>
          </>
        ) : (
          <span className="text-[var(--text-secondary)]">claims mapped —</span>
        )}
      </button>

      {/* Dark code — files under a node's boundary that no claim reads into (the
          code→model direction). Click to list them on the special page. */}
      <button
        type="button"
        onClick={() => onOpenSpecial("dark")}
        className="seg"
        style={sb("color-mix(in srgb, var(--text) 4%, var(--surface-canvas))")}
        title="Dark code — files under a node's boundary that no claim reads into. Click to list them."
      >
        {health == null ? (
          <span className="text-[var(--text-secondary)]">dark code —</span>
        ) : dark > 0 ? (
          <>
            <span className="pl-strong font-medium">{dark}</span>
            <span className="text-[var(--text-secondary)]">dark file{dark === 1 ? "" : "s"}</span>
          </>
        ) : (
          <span className="text-[var(--text-secondary)]">no dark code</span>
        )}
      </button>

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
            className="rseg"
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
              className="rseg"
              style={sb("color-mix(in srgb, var(--color-orange-500) 18%, var(--surface-canvas))")}
              title="Open Needs review — flags awaiting a human verdict (drift, stale claims, agent edits, empty symbols …)"
            >
              <Flag className="h-3 w-3 shrink-0 text-orange-600 dark:text-orange-400" />
              <span className="font-medium text-orange-600 dark:text-orange-400">
                {reviewIndex.total} to review
              </span>
            </button>
          )}
          <button
            type="button"
            onClick={() => onOpenSpecial("changes")}
            className="rseg"
            style={sb("color-mix(in srgb, var(--text) 6.5%, var(--surface-canvas))")}
            title="Changes — the whole plan diff against the committed model, grouped by the ledger's open changes"
          >
            <History className="h-3.5 w-3.5" />
            <span>
              changes
              {(model.changes?.length ?? 0) > 0 && (
                <span
                  className="text-[var(--text-muted)]"
                  title="Open changes in the ledger — planned work not yet folded into the committed model. A change closes when all its work is marked implemented (or reverted)."
                >
                  {" "}
                  · {model.changes!.length} in flight
                </span>
              )}
            </span>
          </button>
        </>
      )}
    </div>
  );
}
