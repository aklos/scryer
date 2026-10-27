/**
 * Landing — what changed since you last looked, in two short blocks, each
 * item one line that opens in place:
 *  - In flight: each git worktree's work against its base branch
 *    (`WorktreesSection`), when the project has several.
 *  - Changed since you looked: one line per agent session active since the
 *    project was last opened, newest first, titled by its task (else the
 *    first component it touched, else its branch and start time), with what
 *    changed state; it opens to the claims its edits newly proved, newly
 *    planned or left planned, the ones now stale or failing, new dark files
 *    and the components it touched. A last line names the components more
 *    than one session touched.
 * Rows open the claim's node page and flash the claim. Computed at read time
 * from the session logs and the derived status (`get_landing`); read-only.
 * With several worktrees each session is read against its own checkout's
 * tree and carries that worktree's branch.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Bot, History, Users } from "lucide-react";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { respElementId } from "../../widgets/source-section/SourceSection";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";
import { CLAIM_LABEL, CLAIM_TONE } from "../../entities/model/claimState";
import type { ScryModel } from "../../entities/model/viewmodel";
import { useAgentIntegration } from "../../features/agent-integration/useAgentIntegration";
import { useWorktrees } from "../../features/worktrees/useWorktrees";
import { WorktreesSection } from "./WorktreesSection";
import { FoldRow } from "./FoldRow";
import { lastLooked, markLooked } from "../../features/derived-tree/lastOpened";

type ClaimState = "proven" | "stale" | "failing" | "planned";

interface LandingClaim {
  respId: string;
  nodeId: string;
  statement: string;
  file: string;
  line: number;
  state: ClaimState;
  /** Concern tags, shown at the row's end. */
  concerns?: string[];
}

interface LandingSession {
  session: string;
  title: string;
  /** The title is the task the agent named when it oriented. */
  titledByTask?: boolean;
  startedAt: number;
  endedAt: number;
  newlyProven: LandingClaim[];
  newlyPlanned: LandingClaim[];
  stillPlanned: LandingClaim[];
  newlyStaleOrFailing: LandingClaim[];
  survivingMutants: number;
  newDarkFiles: string[];
  components: string[];
  /** The worktree the session worked in, when the project has several. */
  worktree?: string;
}

interface LandingWorktree {
  path: string;
  branch?: string;
  current: boolean;
  sessions: number;
}

interface Landing {
  since: number;
  sessions: LandingSession[];
  shared: { nodeId: string; sessions: string[] }[];
  /** The open checkout's branch. */
  branch?: string;
  /** The project's live worktrees; absent for a single checkout. */
  worktrees?: LandingWorktree[];
}

const when = (at: number) =>
  new Date(at * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

const TONE: Record<string, string> = CLAIM_TONE;

export function LandingPage({
  projectPath,
  model,
  onSelectNode,
  onOpenWorktree,
}: {
  projectPath: string | null;
  model: ScryModel;
  onSelectNode: (id: string) => void;
  /** Open another worktree of the project. */
  onOpenWorktree: (path: string) => void;
}) {
  const integration = useAgentIntegration(projectPath);
  const worktrees = useWorktrees(projectPath);
  const [landing, setLanding] = useState<Landing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  // The window is fixed while you look; leaving the page ends the look, so
  // the next visit shows only what came after.
  const [since, setSince] = useState(() => (projectPath ? lastLooked(projectPath) : 0));
  useEffect(() => {
    if (!projectPath) return;
    setSince(lastLooked(projectPath));
    // A look shorter than a glance is StrictMode's mount-unmount-mount, not
    // you leaving; it must not end the look.
    const shown = Date.now();
    const end = () => Date.now() - shown > 1500 && markLooked(projectPath);
    window.addEventListener("beforeunload", end);
    return () => {
      window.removeEventListener("beforeunload", end);
      end();
    };
  }, [projectPath]);
  const window_ = showAll ? 0 : since;

  // A read derives every worktree's tree and can take seconds; only the
  // latest request may land, so a slow earlier one (another window, an
  // event-triggered refresh) never overwrites a newer answer.
  const latest = useRef(0);
  const load = useCallback(() => {
    if (!projectPath) return;
    const request = ++latest.current;
    invoke<Landing>("get_landing", { cwd: projectPath, since: window_ })
      .then((l) => {
        if (request !== latest.current) return;
        setLanding(l);
        setError(null);
      })
      .catch((e) => {
        if (request === latest.current) setError(String(e));
      });
  }, [projectPath, window_]);

  // A new window shows "Loading…" until its own answer arrives, never the
  // previous window's sessions under the new subtitle.
  useEffect(() => setLanding(null), [projectPath, window_]);

  // On open, when the window changes, and as sessions append or reports land.
  useEffect(() => {
    load();
    const offs = [listen("session-changed", load), listen("test-results-changed", load)];
    return () => offs.forEach((p) => void p.then((f) => f()));
  }, [load]);

  const names = useMemo(() => new Map(model.nodes.map((n) => [n.id, n.name])), [model]);
  // A session is named by the task its agent oriented on, quoted as the
  // agent's words; else by the first component it touched; else by its
  // branch and start time. Scryer never records the developer's prompts.
  const titles = useMemo(() => {
    const branchOf = (s: LandingSession) =>
      landing?.worktrees?.find((w) => w.path === s.worktree)?.branch ?? landing?.branch ?? "detached";
    return new Map(
      (landing?.sessions ?? []).map((s) => [s.session, sessionTitle(s, branchOf(s))]),
    );
  }, [landing]);

  // Open the claim's page and flash the claim once it renders.
  const openClaim = (c: Pick<LandingClaim, "respId" | "nodeId">) => {
    onSelectNode(c.nodeId);
    window.setTimeout(() => jumpTo(respElementId(c.respId)), 250);
  };

  // The heading says what the page is; the subtitle only says from when.
  const subtitle = error
    ? error
    : !landing
      ? "Loading…"
      : window_ === 0
        ? `All sessions · ${landing.sessions.length}`
        : since === 0
          ? "First time here"
          : when(since);

  const branchOf = (s: LandingSession) =>
    landing?.worktrees?.find((w) => w.path === s.worktree)?.branch ?? landing?.branch ?? undefined;
  // Only sessions whose work changed a claim's state since the last look; one
  // that only read or edited without moving a claim has nothing to show.
  const sessions = (landing?.sessions ?? []).filter((s) => sessionCounts(s).length > 0);

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Since you last looked"
        subtitle={subtitle}
        right={
          <button
            type="button"
            onClick={() => setShowAll((v) => !v)}
            className="rounded-md px-2 py-1 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)]"
          >
            {showAll ? "Only since last look" : "Show earlier sessions"}
          </button>
        }
      />
      <SpecialBody>
        <div className="[&>:first-child]:!mt-0">
          <WorktreesSection data={worktrees} onOpenWorktree={onOpenWorktree} onOpenClaim={openClaim} />
          <PageSection
            title={window_ === 0 ? "All agent sessions" : "Agent sessions since you looked"}
            hint="Each coding-agent session that worked here, named by the task it oriented on, and what its edits changed the state of. A line opens to its claims."
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
                <p className="text-sm text-[var(--text-muted)]">No agent sessions since you last looked.</p>
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

/** A session's name on the landing: the task its agent oriented on, in
 *  quotes as the agent's words; else "session in" the first component it
 *  touched; else "session on" its branch and start time. */
export function sessionTitle(s: Pick<LandingSession, "title" | "titledByTask" | "startedAt">, branch: string): string {
  if (s.title && s.titledByTask) return `“${s.title}”`;
  if (s.title) return `session in ${s.title}`;
  return `session on ${branch} · ${when(s.startedAt)}`;
}

/** What changed state in a session, as a short phrase per kind; empty when
 *  nothing did. */
export function sessionCounts(s: LandingSession): string[] {
  const todo = s.newlyPlanned.length + s.stillPlanned.length;
  return [
    s.newlyProven.length > 0 && `+${s.newlyProven.length} proven`,
    s.newlyStaleOrFailing.length > 0 && `${s.newlyStaleOrFailing.length} unverified or failing`,
    todo > 0 && `${todo} todo`,
    s.survivingMutants > 0 && `${s.survivingMutants} surviving mutant${s.survivingMutants === 1 ? "" : "s"}`,
    s.newDarkFiles.length > 0 && `${s.newDarkFiles.length} new dark file${s.newDarkFiles.length === 1 ? "" : "s"}`,
  ].filter((x): x is string => typeof x === "string");
}

function SessionRow({
  s,
  title,
  names,
  branch,
  onOpenClaim,
  onSelectNode,
}: {
  s: LandingSession;
  title: string;
  names: Map<string, string>;
  /** The worktree's branch, when the project has several. */
  branch?: string;
  onOpenClaim: (c: LandingClaim) => void;
  onSelectNode: (id: string) => void;
}) {
  const groups: [string, LandingClaim[]][] = [
    ["Newly proven", s.newlyProven],
    ["Newly unverified or failing", s.newlyStaleOrFailing],
    ["New todo", s.newlyPlanned],
    ["Still todo", s.stillPlanned],
  ];
  const counts = sessionCounts(s);
  const detail = groups.some(([, rows]) => rows.length > 0) || s.newDarkFiles.length > 0 || s.components.length > 0;
  return (
    <FoldRow
      summary={
        <span className="flex min-w-0 flex-1 items-baseline gap-2">
          <Bot className="h-3.5 w-3.5 shrink-0 self-center text-[var(--text-ghost)]" />
          <span
            className="min-w-0 flex-1 truncate text-[var(--text)]"
            title={s.titledByTask ? `Agent session ${s.session.slice(0, 8)} — the task it oriented on` : `Agent session ${s.session.slice(0, 8)}`}
          >
            {title}
          </span>
          {branch && <span className="shrink-0 font-mono text-xs text-[var(--text-tertiary)]">{branch}</span>}
          <span className="shrink-0 text-xs text-[var(--text-muted)]">{counts.join(" · ")}</span>
          <span className="shrink-0 text-xs text-[var(--text-ghost)]">{when(s.endedAt)}</span>
        </span>
      }
    >
      {detail && (
        <>
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
        </>
      )}
    </FoldRow>
  );
}
