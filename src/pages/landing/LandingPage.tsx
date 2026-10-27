/**
 * Sessions — the coding agents working here, and the ones that did lately.
 * Each session is one line: what it is on (the task it oriented on), where
 * it is working now or when it ended, and the claims it moved — proven,
 * written as todo, now failing. A line opens to those claims, each opening
 * its node page. A live session can be followed: the wiki and the map then
 * go where it edits. Read from the session logs and the derived status
 * (`get_landing`); read-only.
 */

import { useMemo, useState } from "react";
import { Bot, Eye } from "lucide-react";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { respElementId } from "../../widgets/source-section/SourceSection";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";
import { CLAIM_LABEL, CLAIM_TONE } from "../../entities/model/claimState";
import type { ScryModel } from "../../entities/model/viewmodel";
import { useAgentIntegration } from "../../features/agent-integration/useAgentIntegration";
import {
  followTarget,
  RECENT_SECS,
  sessionCounts,
  sessionTitle,
  useSessions,
  type LoggedSession,
  type SessionClaim,
} from "../../features/sessions/useSessions";
import { FoldRow } from "./FoldRow";

const TONE: Record<string, string> = CLAIM_TONE;

/** "just now", "12m ago", "3h ago", else the day. */
export function ago(at: number, now = Date.now() / 1000): string {
  const s = Math.max(0, now - at);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return new Date(at * 1000).toLocaleDateString([], { month: "short", day: "numeric" });
}

export function LandingPage({
  projectPath,
  model,
  following,
  onFollow,
  onSelectNode,
}: {
  projectPath: string | null;
  model: ScryModel;
  /** The session the wiki and map follow, if any. */
  following: string | null;
  /** Follow a session, or stop with null. */
  onFollow: (session: string | null) => void;
  onSelectNode: (id: string) => void;
}) {
  const integration = useAgentIntegration(projectPath);
  const [showAll, setShowAll] = useState(false);
  // The week back from when the page opened: fixed while it is open, so a
  // re-read lands on the same window.
  const [recent] = useState(() => Math.floor(Date.now() / 1000) - RECENT_SECS);
  const { data, error } = useSessions(projectPath, showAll ? 0 : recent);

  const names = useMemo(() => new Map(model.nodes.map((n) => [n.id, n.name])), [model]);
  const several = (data?.worktrees?.length ?? 0) > 1;
  const branchOf = (s: LoggedSession) =>
    data?.worktrees?.find((w) => w.path === s.worktree)?.branch ?? data?.branch ?? "detached";

  // Open the claim's page and flash the claim once it renders.
  const openClaim = (c: Pick<SessionClaim, "respId" | "nodeId">) => {
    onSelectNode(c.nodeId);
    window.setTimeout(() => jumpTo(respElementId(c.respId)), 250);
  };

  const live = (data?.sessions ?? []).filter((s) => s.live);
  // An ended session shows only when it moved a claim.
  const ended = (data?.sessions ?? []).filter((s) => !s.live && sessionCounts(s).length > 0);

  const row = (s: LoggedSession) => (
    <SessionRow
      key={s.session}
      s={s}
      title={sessionTitle(s, branchOf(s))}
      branch={several ? branchOf(s) : undefined}
      names={names}
      following={following === s.session}
      onFollow={onFollow}
      onOpenClaim={openClaim}
    />
  );

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Sessions"
        subtitle={
          error ??
          (!data
            ? "Loading…"
            : live.length === 0
              ? "No agent is working here now"
              : `${live.length} agent${live.length === 1 ? "" : "s"} working now`)
        }
      />
      <SpecialBody>
        <div className="[&>:first-child]:!mt-0">
          <PageSection title="Working now" count={data ? live.length : undefined}>
            {integration.status && !integration.status.on && data?.sessions.length === 0 && (
              <p className="mb-1 text-xs text-[var(--text-muted)]">
                Agent integration is off for this project, so no sessions are recorded. Turn it on in Settings, or run{" "}
                <code className="font-mono text-[var(--text-secondary)]">scryer-mcp setup on</code>.
              </p>
            )}
            {data && live.length === 0 ? (
              <p className="py-1 text-sm text-[var(--text-muted)]">Nothing running. Start an agent and it shows up here.</p>
            ) : (
              <ul className="flex flex-col">{live.map(row)}</ul>
            )}
          </PageSection>
          {data && (ended.length > 0 || !showAll) && (
            <PageSection title="Earlier" count={ended.length}>
              <ul className="flex flex-col">{ended.map(row)}</ul>
              <button
                type="button"
                onClick={() => setShowAll((v) => !v)}
                className="mt-1 px-1 text-xs text-[var(--text-muted)] hover:underline"
              >
                {showAll ? "Only the last 7 days" : "Older sessions"}
              </button>
            </PageSection>
          )}
        </div>
      </SpecialBody>
    </div>
  );
}

function SessionRow({
  s,
  title,
  branch,
  names,
  following,
  onFollow,
  onOpenClaim,
}: {
  s: LoggedSession;
  title: string;
  /** The worktree's branch, when the project has several. */
  branch?: string;
  names: Map<string, string>;
  following: boolean;
  onFollow: (session: string | null) => void;
  onOpenClaim: (c: SessionClaim) => void;
}) {
  const failing = s.newlyStaleOrFailing.filter((c) => c.state === "failing");
  const claims = [...failing, ...s.newlyProven, ...s.newlyPlanned];
  const here = followTarget(s);
  const where = s.live ? (here ? `in ${names.get(here) ?? here}` : "starting") : `ended ${ago(s.endedAt)}`;
  return (
    <FoldRow
      aside={
        s.live && (
          <button
            type="button"
            aria-pressed={following}
            onClick={() => onFollow(following ? null : s.session)}
            title={following ? "Stop following this session" : "Follow this session: the wiki and map go where it edits"}
            className={`flex shrink-0 items-center gap-1 rounded-md px-2 py-0.5 text-xs font-medium ${
              following
                ? "bg-violet-600 text-white dark:bg-violet-500"
                : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
            }`}
          >
            <Eye className="h-3.5 w-3.5" />
            {following ? "Following" : "Follow"}
          </button>
        )
      }
      summary={
        <span className="flex min-w-0 flex-1 items-baseline gap-2">
          {s.live ? (
            <span className="h-2 w-2 shrink-0 self-center animate-pulse rounded-full bg-violet-600 dark:bg-violet-500" />
          ) : (
            <Bot className="h-3.5 w-3.5 shrink-0 self-center text-[var(--text-ghost)]" />
          )}
          <span className="min-w-0 flex-1 truncate text-[var(--text)]" title={`Agent session ${s.session.slice(0, 8)}`}>
            {title}
          </span>
          {branch && <span className="shrink-0 font-mono text-xs text-[var(--text-tertiary)]">{branch}</span>}
          <span className="shrink-0 text-xs text-[var(--text-muted)]">{sessionCounts(s).join(" · ")}</span>
          <span className="w-28 shrink-0 truncate text-right text-xs text-[var(--text-ghost)]">{where}</span>
        </span>
      }
    >
      {claims.length > 0 && (
        <ul className="flex flex-col">
          {claims.map((c) => (
            <li key={c.respId}>
              <button
                type="button"
                onClick={() => onOpenClaim(c)}
                className="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left text-sm hover:bg-[var(--surface-hover)]"
                title={`${c.file}:${c.line}`}
              >
                <span className={`w-14 shrink-0 text-xs ${TONE[c.state] ?? ""}`}>{CLAIM_LABEL[c.state]}</span>
                <span className="min-w-0 flex-1 truncate text-[var(--text)]">{c.statement}</span>
                <span className="shrink-0 text-xs text-[var(--text-ghost)]">{names.get(c.nodeId) ?? ""}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </FoldRow>
  );
}
