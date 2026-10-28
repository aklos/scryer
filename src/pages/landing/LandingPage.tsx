/**
 * Sessions — the coding agents working here, and the ones that did lately.
 * Each section is a table, a session per row: what it is on (the task it
 * oriented on), the claims it moved — proven, failing, written as todo — a
 * column each, and when it started or ended. A row opens to those claims,
 * each opening its node page. A live session can be followed: the wiki and
 * the map then go where it edits. Read from the session logs and the derived status
 * (`get_landing`); read-only.
 */

import { useMemo, useState } from "react";
import { Bot, ChevronRight, Eye } from "lucide-react";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { respElementId } from "../../widgets/source-section/SourceSection";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";
import { CLAIM_LABEL, CLAIM_TONE } from "../../entities/model/claimState";
import type { ScryModel } from "../../entities/model/viewmodel";
import { useAgentIntegration } from "../../features/agent-integration/useAgentIntegration";
import {
  RECENT_SECS,
  sessionCounts,
  sessionTitle,
  useSessions,
  type LoggedSession,
  type SessionClaim,
} from "../../features/sessions/useSessions";

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
  // The branch the session worked on, else its checkout's branch now.
  const branchOf = (s: LoggedSession) =>
    s.branch ?? data?.worktrees?.find((w) => w.path === s.worktree)?.branch ?? data?.branch ?? "detached";

  // Open the claim's page and flash the claim once it renders.
  const openClaim = (c: Pick<SessionClaim, "respId" | "nodeId">) => {
    onSelectNode(c.nodeId);
    window.setTimeout(() => jumpTo(respElementId(c.respId)), 250);
  };

  // A branch column only when sessions are on more than one branch.
  const several = new Set((data?.sessions ?? []).map(branchOf)).size > 1 || (data?.worktrees?.length ?? 0) > 1;
  const live = (data?.sessions ?? []).filter((s) => s.live);
  // An ended session shows only when it moved a claim.
  const ended = (data?.sessions ?? []).filter((s) => !s.live && sessionCounts(s).length > 0);

  const table = (sessions: LoggedSession[], isLive: boolean) => (
    <SessionTable
      sessions={sessions}
      live={isLive}
      followable={live.length > 0}
      titleOf={(s) => sessionTitle(s, branchOf(s))}
      branchOf={several ? branchOf : undefined}
      names={names}
      following={following}
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
              live.length > 0 && table(live, true)
            )}
          </PageSection>
          {data && (ended.length > 0 || !showAll) && (
            <PageSection title="Earlier" count={ended.length}>
              {ended.length > 0 && table(ended, false)}
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

/** The claim counts a session row shows, one column each. */
const COUNTS = [
  { state: "proven", label: "Proven", of: (s: LoggedSession) => s.newlyProven.length },
  { state: "failing", label: "Failing", of: (s: LoggedSession) => failingOf(s).length },
  { state: "planned", label: "Todo", of: (s: LoggedSession) => s.newlyPlanned.length },
] as const;

const failingOf = (s: LoggedSession) => s.newlyStaleOrFailing.filter((c) => c.state === "failing");

const HEAD = "px-2 pb-1 text-left text-[11px] font-medium text-[var(--text-ghost)]";
// Fixed widths for every column but the task's, so both sections' columns line up.
const W_MARK = "w-5";
const W_BRANCH = "w-20";
const W_COUNT = "w-14";
const W_WHEN = "w-20";
const W_FOLLOW = "w-24";
const CELL = "px-2 py-1 align-baseline";

/** One section's sessions, a row each; a row opens to the claims it moved. */
function SessionTable({
  sessions,
  live,
  followable,
  titleOf,
  branchOf,
  names,
  following,
  onFollow,
  onOpenClaim,
}: {
  sessions: LoggedSession[];
  live: boolean;
  /** A session on the page is live: both sections keep a column for Follow. */
  followable: boolean;
  titleOf: (s: LoggedSession) => string;
  /** The worktree's branch, when the project has several. */
  branchOf?: (s: LoggedSession) => string;
  names: Map<string, string>;
  following: string | null;
  onFollow: (session: string | null) => void;
  onOpenClaim: (c: SessionClaim) => void;
}) {
  const columns = 4 + (branchOf ? 1 : 0) + COUNTS.length + (followable ? 1 : 0);
  return (
    <table className="w-full table-fixed border-collapse text-sm">
      <colgroup>
        <col className={W_MARK} />
        <col className={W_MARK} />
        <col />
        {branchOf && <col className={W_BRANCH} />}
        {COUNTS.map((c) => (
          <col key={c.state} className={W_COUNT} />
        ))}
        <col className={W_WHEN} />
        {followable && <col className={W_FOLLOW} />}
      </colgroup>
      <thead>
        <tr className="border-b border-[var(--border)]">
          <th className={HEAD} colSpan={2} />
          <th className={HEAD}>Task</th>
          {branchOf && <th className={HEAD}>Branch</th>}
          {COUNTS.map((c) => (
            <th key={c.state} className={`${HEAD} text-right`}>
              {c.label}
            </th>
          ))}
          <th className={`${HEAD} text-right`}>{live ? "Started" : "Ended"}</th>
          {followable && <th className={HEAD} />}
        </tr>
      </thead>
      {sessions.map((s) => (
        <SessionRows
          key={s.session}
          s={s}
          title={titleOf(s)}
          branch={branchOf?.(s)}
          columns={columns}
          followable={followable}
          names={names}
          following={following === s.session}
          onFollow={onFollow}
          onOpenClaim={onOpenClaim}
        />
      ))}
    </table>
  );
}

function SessionRows({
  s,
  title,
  branch,
  columns,
  followable,
  names,
  following,
  onFollow,
  onOpenClaim,
}: {
  s: LoggedSession;
  title: string;
  branch?: string;
  columns: number;
  followable: boolean;
  names: Map<string, string>;
  following: boolean;
  onFollow: (session: string | null) => void;
  onOpenClaim: (c: SessionClaim) => void;
}) {
  const [open, setOpen] = useState(false);
  const claims = [...failingOf(s), ...s.newlyProven, ...s.newlyPlanned];
  const folds = claims.length > 0;
  const when = ago(s.live ? s.startedAt : s.endedAt);
  return (
    <tbody>
      <tr
        onClick={folds ? () => setOpen((v) => !v) : undefined}
        className={`border-b border-[var(--border-subtle)] ${folds ? "cursor-pointer hover:bg-[var(--surface-hover)]" : ""}`}
      >
        <td className="py-1 pl-1 align-middle">
          <button
            type="button"
            aria-expanded={folds ? open : undefined}
            aria-label={open ? "Hide claims" : "Show claims"}
            disabled={!folds}
            onClick={(e) => {
              e.stopPropagation();
              setOpen((v) => !v);
            }}
            className="flex items-center"
          >
            <ChevronRight
              className={`h-3.5 w-3.5 text-[var(--text-ghost)] transition-transform ${open ? "rotate-90" : ""} ${folds ? "" : "invisible"}`}
            />
          </button>
        </td>
        <td className="py-1 pl-1 align-middle">
          {s.live ? (
            <span className="block h-2 w-2 animate-pulse rounded-full bg-violet-600 dark:bg-violet-500" />
          ) : (
            <Bot className="h-3.5 w-3.5 text-[var(--text-ghost)]" />
          )}
        </td>
        <td className={`${CELL} truncate text-[var(--text)]`} title={`${title} — agent session ${s.session.slice(0, 8)}`}>
          {title}
        </td>
        {branch !== undefined && (
          <td className={`${CELL} truncate font-mono text-xs text-[var(--text-tertiary)]`}>{branch}</td>
        )}
        {COUNTS.map((c) => {
          const n = c.of(s);
          return (
            <td key={c.state} className={`${CELL} text-right text-xs tabular-nums ${n > 0 ? TONE[c.state] : ""}`}>
              {n > 0 ? n : ""}
            </td>
          );
        })}
        <td className={`${CELL} truncate text-right text-xs text-[var(--text-ghost)]`}>{when}</td>
        {followable && (
          <td className="py-1 pl-2 align-middle">
            {s.live && (
              <button
                type="button"
                aria-pressed={following}
                onClick={(e) => {
                  e.stopPropagation();
                  onFollow(following ? null : s.session);
                }}
                title={following ? "Stop following this session" : "Follow this session: the wiki and map go where it edits"}
                className={`flex items-center gap-1 whitespace-nowrap rounded-md px-2 py-0.5 text-xs font-medium ${
                  following
                    ? "bg-violet-600 text-white dark:bg-violet-500"
                    : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                }`}
              >
                <Eye className="h-3.5 w-3.5" />
                {following ? "Following" : "Follow"}
              </button>
            )}
          </td>
        )}
      </tr>
      {folds && open && (
        <tr>
          <td colSpan={columns} className="pb-2 pl-10 pt-0.5">
            <table className="w-full border-collapse text-sm">
              <tbody>
                {claims.map((c) => (
                  <tr
                    key={c.respId}
                    tabIndex={0}
                    onClick={() => onOpenClaim(c)}
                    onKeyDown={(e) => e.key === "Enter" && onOpenClaim(c)}
                    title={`${c.file}:${c.line}`}
                    className="cursor-pointer hover:bg-[var(--surface-hover)] focus-visible:outline focus-visible:outline-1 focus-visible:outline-[var(--accent)]"
                  >
                    <td className={`w-0 whitespace-nowrap py-0.5 pl-1 pr-3 align-baseline text-xs ${TONE[c.state] ?? ""}`}>
                      {CLAIM_LABEL[c.state]}
                    </td>
                    <td className="w-full max-w-0 truncate py-0.5 align-baseline text-[var(--text)]">
                      {c.statement}
                    </td>
                    <td className="whitespace-nowrap py-0.5 pl-3 pr-1 text-right align-baseline text-xs text-[var(--text-ghost)]">
                      {names.get(c.nodeId) ?? ""}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </td>
        </tr>
      )}
    </tbody>
  );
}
