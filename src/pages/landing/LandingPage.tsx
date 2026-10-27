/**
 * Sessions — the agent sessions working here and what they did, newest
 * first with the live ones on top, each one line that opens in place:
 *  - In flight: each git worktree's work against its base branch
 *    (`WorktreesSection`), when the project has several.
 *  - Agent sessions: one line per session in the last week (all of them on
 *    asking), titled by its task (else the first component it touched,
 *    else its branch and start time), with what changed state. A live
 *    session opens on its feed — the files it edits, the todo tests it
 *    writes, the verdicts its claims get, newest first — and can be
 *    followed: the wiki and the map then go where it edits. An ended one
 *    opens to the claims its edits newly proved, newly planned or left
 *    planned, the ones now stale or failing, new dark files, the components
 *    it touched, and its feed. A last line names the components more than
 *    one session touched.
 * Rows open the claim's node page and flash the claim. Computed at read time
 * from the session logs and the derived status (`get_landing`); read-only.
 * With several worktrees each session is read against its own checkout's
 * tree and carries that worktree's branch.
 */

import { useMemo, useState } from "react";
import { Bot, Eye, History, Users } from "lucide-react";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { respElementId } from "../../widgets/source-section/SourceSection";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";
import { CLAIM_LABEL, CLAIM_TONE } from "../../entities/model/claimState";
import type { ScryModel } from "../../entities/model/viewmodel";
import { useAgentIntegration } from "../../features/agent-integration/useAgentIntegration";
import { useWorktrees } from "../../features/worktrees/useWorktrees";
import {
  RECENT_SECS,
  sessionCounts,
  sessionTitle,
  stepLine,
  useSessions,
  when,
  type LoggedSession,
  type SessionClaim,
  type SessionStep,
} from "../../features/sessions/useSessions";
import { WorktreesSection } from "./WorktreesSection";
import { FoldRow } from "./FoldRow";

const TONE: Record<string, string> = CLAIM_TONE;

/** How many of a session's latest steps its feed shows before "all steps". */
const FEED_STEPS = 12;

export function LandingPage({
  projectPath,
  model,
  following,
  onFollow,
  onSelectNode,
  onOpenWorktree,
}: {
  projectPath: string | null;
  model: ScryModel;
  /** The session the wiki and map follow, if any. */
  following: string | null;
  /** Follow a session, or stop with null. */
  onFollow: (session: string | null) => void;
  onSelectNode: (id: string) => void;
  /** Open another worktree of the project. */
  onOpenWorktree: (path: string) => void;
}) {
  const integration = useAgentIntegration(projectPath);
  const worktrees = useWorktrees(projectPath);
  const [showAll, setShowAll] = useState(false);
  // The week back from when the page opened: fixed while it is open, so a
  // re-read lands on the same window.
  const [recent] = useState(() => Math.floor(Date.now() / 1000) - RECENT_SECS);
  const { data: landing, error } = useSessions(projectPath, showAll ? 0 : recent);

  const names = useMemo(() => new Map(model.nodes.map((n) => [n.id, n.name])), [model]);
  const branchOf = (s: LoggedSession) =>
    landing?.worktrees?.find((w) => w.path === s.worktree)?.branch ?? landing?.branch ?? undefined;
  // A session is named by the task its agent oriented on, quoted as the
  // agent's words; else by the first component it touched; else by its
  // branch and start time. Scryer never records the developer's prompts.
  const titles = new Map((landing?.sessions ?? []).map((s) => [s.session, sessionTitle(s, branchOf(s) ?? "detached")]));

  // Open the claim's page and flash the claim once it renders.
  const openClaim = (c: Pick<SessionClaim, "respId" | "nodeId">) => {
    onSelectNode(c.nodeId);
    window.setTimeout(() => jumpTo(respElementId(c.respId)), 250);
  };

  // A live session shows from its start; an ended one only when it did
  // something there is to show.
  const sessions = (landing?.sessions ?? []).filter(
    (s) => s.live || (s.steps ?? []).length > 0 || sessionCounts(s).length > 0,
  );
  const live = sessions.filter((s) => s.live).length;

  const subtitle = error
    ? error
    : !landing
      ? "Loading…"
      : [live > 0 && `${live} live`, showAll ? `All sessions · ${sessions.length}` : `Last 7 days · ${sessions.length}`]
          .filter(Boolean)
          .join(" · ");

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Sessions"
        subtitle={subtitle}
        right={
          <button
            type="button"
            onClick={() => setShowAll((v) => !v)}
            className="rounded-md px-2 py-1 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)]"
          >
            {showAll ? "Only the last 7 days" : "Show older sessions"}
          </button>
        }
      />
      <SpecialBody>
        <div className="[&>:first-child]:!mt-0">
          <WorktreesSection data={worktrees} onOpenWorktree={onOpenWorktree} onOpenClaim={openClaim} />
          <PageSection
            title="Agent sessions"
            hint="Each coding-agent session that worked here, named by the task it oriented on. A live one shows what it is doing as it works; follow it and the wiki and map go where it edits."
            count={landing ? sessions.length : undefined}
          >
            {integration.status && !integration.status.on && landing?.sessions.length === 0 && (
              <p className="mb-1 text-xs text-[var(--text-muted)]">
                Agent integration is off for this project, so no sessions are recorded. Turn it on in Settings, or run{" "}
                <code className="font-mono text-[var(--text-secondary)]">scryer-mcp setup on</code>.
              </p>
            )}
            {landing && sessions.length === 0 ? (
              <div className="flex items-center gap-2 py-1">
                <History className="h-4 w-4 text-[var(--text-ghost)]" />
                <p className="text-sm text-[var(--text-muted)]">
                  {showAll ? "No agent sessions yet." : "No agent sessions in the last 7 days."}
                </p>
              </div>
            ) : (
              <ul className="flex flex-col">
                {sessions.map((s) => (
                  <SessionRow
                    key={s.session}
                    s={s}
                    title={titles.get(s.session) ?? s.title}
                    names={names}
                    branch={(landing?.worktrees?.length ?? 0) > 1 ? branchOf(s) ?? "detached" : undefined}
                    following={following === s.session}
                    onFollow={onFollow}
                    onOpenClaim={openClaim}
                    onSelectNode={onSelectNode}
                  />
                ))}
                {landing && landing.shared.length > 0 && (
                  <FoldRow
                    summary={
                      <span className="flex min-w-0 flex-1 items-center gap-2 text-[var(--text-secondary)]">
                        <Users className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
                        {landing.shared.length} component{landing.shared.length === 1 ? "" : "s"} touched by more than one session
                      </span>
                    }
                  >
                    <ul className="flex flex-col gap-0.5">
                      {landing.shared.map((c) => (
                        <li key={c.nodeId} className="flex items-baseline gap-2 text-sm">
                          <button type="button" className="text-[var(--accent)] hover:underline" onClick={() => onSelectNode(c.nodeId)}>
                            {names.get(c.nodeId) ?? c.nodeId}
                          </button>
                          <span className="truncate text-xs text-[var(--text-muted)]">
                            {c.sessions.map((s) => titles.get(s) ?? s.slice(0, 8)).join(" · ")}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </FoldRow>
                )}
              </ul>
            )}
          </PageSection>
        </div>
      </SpecialBody>
    </div>
  );
}

function SessionRow({
  s,
  title,
  names,
  branch,
  following,
  onFollow,
  onOpenClaim,
  onSelectNode,
}: {
  s: LoggedSession;
  title: string;
  names: Map<string, string>;
  /** The worktree's branch, when the project has several. */
  branch?: string;
  following: boolean;
  onFollow: (session: string | null) => void;
  onOpenClaim: (c: SessionClaim) => void;
  onSelectNode: (id: string) => void;
}) {
  const groups: [string, SessionClaim[]][] = [
    ["Newly proven", s.newlyProven],
    ["Newly unverified or failing", s.newlyStaleOrFailing],
    ["New todo", s.newlyPlanned],
    ["Still todo", s.stillPlanned],
  ];
  const counts = sessionCounts(s);
  const steps = s.steps ?? [];
  const detail =
    s.live || steps.length > 0 || groups.some(([, rows]) => rows.length > 0) || s.newDarkFiles.length > 0 || s.components.length > 0;
  return (
    <FoldRow
      defaultOpen={s.live}
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
            <span
              className="h-2 w-2 shrink-0 self-center animate-pulse rounded-full bg-violet-600 dark:bg-violet-500"
              title="Working now"
            />
          ) : (
            <Bot className="h-3.5 w-3.5 shrink-0 self-center text-[var(--text-ghost)]" />
          )}
          <span
            className="min-w-0 flex-1 truncate text-[var(--text)]"
            title={s.titledByTask ? `Agent session ${s.session.slice(0, 8)} — the task it oriented on` : `Agent session ${s.session.slice(0, 8)}`}
          >
            {title}
          </span>
          {branch && <span className="shrink-0 font-mono text-xs text-[var(--text-tertiary)]">{branch}</span>}
          <span className="shrink-0 text-xs text-[var(--text-muted)]">{counts.join(" · ")}</span>
          <span className="shrink-0 text-xs text-[var(--text-ghost)]">{s.live ? "live" : when(s.endedAt)}</span>
        </span>
      }
    >
      {detail && (
        <>
          {s.live && <SessionFeed steps={steps} names={names} onOpenClaim={onOpenClaim} onSelectNode={onSelectNode} />}
          {groups
            .filter(([, rows]) => rows.length > 0)
            .map(([label, rows]) => (
              <div key={label}>
                <div className="text-[11px] font-medium uppercase tracking-wide text-[var(--text-tertiary)]">{label}</div>
                <ul className="flex flex-col">
                  {rows.map((c) => (
                    <li key={c.respId}>
                      <button
                        type="button"
                        onClick={() => onOpenClaim(c)}
                        className="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left text-sm hover:bg-[var(--surface-hover)]"
                        title={`${c.file}:${c.line}`}
                      >
                        <span className={`shrink-0 text-xs ${TONE[c.state] ?? ""}`}>{CLAIM_LABEL[c.state]}</span>
                        <span className="min-w-0 flex-1 truncate text-[var(--text)]">
                          {c.statement}
                          {c.concerns?.map((t) => (
                            <span key={t} className="ml-1.5 text-xs text-[var(--text-tertiary)]">
                              [{t}]
                            </span>
                          ))}
                        </span>
                        <span className="shrink-0 text-xs text-[var(--text-ghost)]">{names.get(c.nodeId) ?? ""}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          {s.newDarkFiles.length > 0 && <div className="text-xs text-[var(--text-muted)]">Dark: {s.newDarkFiles.join(", ")}</div>}
          {s.components.length > 0 && (
            <div className="text-xs text-[var(--text-muted)]">
              Touched{" "}
              {s.components.map((id, i) => (
                <span key={id}>
                  {i > 0 && " · "}
                  <button type="button" onClick={() => onSelectNode(id)} className="hover:text-[var(--text)] hover:underline">
                    {names.get(id) ?? id}
                  </button>
                </span>
              ))}
            </div>
          )}
          {!s.live && steps.length > 0 && (
            <SessionFeed steps={steps} names={names} onOpenClaim={onOpenClaim} onSelectNode={onSelectNode} />
          )}
        </>
      )}
    </FoldRow>
  );
}

/** A session's steps, newest first: the latest few, all on asking. An edit
 *  opens its component, a todo or verdict its claim. */
function SessionFeed({
  steps,
  names,
  onOpenClaim,
  onSelectNode,
}: {
  steps: SessionStep[];
  names: Map<string, string>;
  onOpenClaim: (c: SessionClaim) => void;
  onSelectNode: (id: string) => void;
}) {
  const [all, setAll] = useState(false);
  const newest = [...steps].reverse();
  const shown = all ? newest : newest.slice(0, FEED_STEPS);
  const name = (id: string) => names.get(id) ?? id;
  const time = (at: number) => new Date(at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (steps.length === 0) return <div className="text-xs text-[var(--text-muted)]">Nothing yet — it has not edited a file.</div>;
  return (
    <div>
      <div className="text-[11px] font-medium uppercase tracking-wide text-[var(--text-tertiary)]">Feed</div>
      <ul className="flex flex-col">
        {shown.map((step, i) => {
          const nodeId = step.kind === "edit" ? step.nodeId : undefined;
          const open = nodeId
            ? () => onSelectNode(nodeId)
            : step.kind === "todo" || step.kind === "verdict"
              ? () => onOpenClaim(step.claim)
              : undefined;
          const tone =
            step.kind === "verdict" ? TONE[step.claim.state] : step.kind === "todo" ? TONE.planned : "text-[var(--text-secondary)]";
          return (
            <li key={`${step.at}-${i}`}>
              <button
                type="button"
                disabled={!open}
                onClick={open}
                className="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left text-sm enabled:hover:bg-[var(--surface-hover)]"
              >
                <span className="shrink-0 font-mono text-2xs tabular-nums text-[var(--text-ghost)]">{time(step.at)}</span>
                <span className={`min-w-0 flex-1 truncate ${tone}`}>{stepLine(step, name)}</span>
              </button>
            </li>
          );
        })}
      </ul>
      {newest.length > FEED_STEPS && (
        <button type="button" onClick={() => setAll((v) => !v)} className="px-1 text-xs text-[var(--text-muted)] hover:underline">
          {all ? "Latest steps only" : `All ${newest.length} steps`}
        </button>
      )}
    </div>
  );
}
