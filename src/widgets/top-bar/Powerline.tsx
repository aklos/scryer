/**
 * The powerline status bar — chevron-tiled segments along the window's bottom
 * edge. Left side reads the tree's standing at a glance: the subagent launch
 * setup, the claims by derived status, dark files, and the tree's size. Right
 * side carries the agent's live activity with Cancel while a fixture fill
 * runs.
 *
 * Segment shades come from the theme surface vars via an inline `--sb`, which the
 * CSS reuses for each chevron's triangle so the tiling stays seamless (see the
 * `.pl` block in index.css).
 */

import type { CSSProperties } from "react";
import { Loader2, X } from "lucide-react";
import { AgentMark } from "../../shared/ui/pagekit";
import type { AgentSession } from "../../features/agent-launch/useAgentSession";
import { darkBoundaries, type ModelHealthReport } from "../../entities/model/health";
import { AGENT_LABEL, type ResolvedLaunch } from "../../features/agent-launch/launchSettings";
import type { ScryModel } from "../../entities/model/viewmodel";
import type { SpecialPage } from "../../entities/model/navigation";

interface PowerlineProps {
  model: ScryModel;
  agent: AgentSession;
  /** Claims mapped, dark files, claim states; null until the first fetch. */
  health: ModelHealthReport | null;
  /** The subagent launch setup a fixture fill will run with. */
  launch: ResolvedLaunch;
  onOpenSpecial: (page: SpecialPage) => void;
  onOpenSettings: () => void;
}

/** Inline `--sb` carrier — drives both a segment's fill and its chevron edge. */
const sb = (value: string): CSSProperties => ({ ["--sb" as string]: value }) as CSSProperties;

export function Powerline({ model, agent, health, launch, onOpenSpecial, onOpenSettings }: PowerlineProps) {
  // Code no test reaches by import.
  const dark = darkBoundaries(health).total;
  const counts = health?.derivedStatus.totals;

  const busy = agent.running;
  const agentLabel = agent.label;
  const agentActivity = agent.activity;

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

      {/* Claims by derived status. Click for what changed since you last
          looked. */}
      <button
        type="button"
        onClick={() => onOpenSpecial("session")}
        className="seg"
        style={sb("color-mix(in srgb, var(--text) 4%, var(--surface-canvas))")}
        title="Claims by derived status — proven, stale, failing, planned. Click for what changed since you last looked."
      >
        {counts ? (
          <>
            <span className="pl-strong font-medium">{counts.proven}</span>
            <span className="text-[var(--text-secondary)]">proven</span>
            <span className="text-[var(--text-tertiary)]">
              · {counts.stale} stale · {counts.failing} failing · {counts.planned} planned
            </span>
          </>
        ) : (
          <span className="text-[var(--text-secondary)]">claims —</span>
        )}
      </button>

      {/* Dark code — files under a node's boundary that no claim reads into (the
          code→model direction). Click to list them on the special page. */}
      <button
        type="button"
        onClick={() => onOpenSpecial("dark")}
        className="seg"
        style={sb("color-mix(in srgb, var(--text) 4%, var(--surface-canvas))")}
        title="Dark code — product files no test reaches by import. Click to list them."
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
            onClick={agent.cancel}
            className="rseg"
            style={sb("color-mix(in srgb, var(--text) 9%, var(--surface-canvas))")}
          >
            <X className="h-3 w-3" />
            cancel
          </button>
        </>
      ) : null}
    </div>
  );
}
