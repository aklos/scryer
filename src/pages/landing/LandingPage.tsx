/**
 * Landing — what changed since you last looked. One card per agent session
 * active since the project was last opened, newest first, titled by its task
 * (else the first component it touched, else its branch and start time):
 * the claims its edits newly proved, newly planned or left planned, the ones
 * now stale or failing, surviving mutants and new dark files in what it
 * touched. Beside them, the components more than one session touched. Rows
 * open the claim's node page and flash the claim. Computed at read time from
 * the session logs and the derived status (`get_landing`); read-only. With
 * several git worktrees, sessions are grouped by the worktree they worked
 * in, each read against that checkout's own tree, and the shared-component
 * count spans them all.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { GitBranch, History, Users } from "lucide-react";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { respElementId } from "../../widgets/source-section/SourceSection";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";
import type { ScryModel } from "../../entities/model/viewmodel";

type ClaimState = "proven" | "stale" | "failing" | "planned";

interface LandingClaim {
  respId: string;
  nodeId: string;
  statement: string;
  file: string;
  line: number;
  state: ClaimState;
}

interface LandingSession {
  session: string;
  title: string;
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

const TONE: Record<string, string> = {
  proven: "text-emerald-700 dark:text-emerald-300",
  planned: "text-sky-700 dark:text-sky-300",
  stale: "text-amber-700 dark:text-amber-300",
  failing: "text-red-700 dark:text-red-300",
};

export function LandingPage({
  projectPath,
  since,
  model,
  onSelectNode,
}: {
  projectPath: string | null;
  /** When the project was last opened before now (unix seconds). */
  since: number;
  model: ScryModel;
  onSelectNode: (id: string) => void;
}) {
  const [landing, setLanding] = useState<Landing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const window_ = showAll ? 0 : since;

  const load = useCallback(() => {
    if (!projectPath) return;
    invoke<Landing>("get_landing", { cwd: projectPath, since: window_ })
      .then((l) => {
        setLanding(l);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, [projectPath, window_]);

  // On open, when the window changes, and as sessions append or reports land.
  useEffect(() => {
    load();
    const offs = [listen("session-changed", load), listen("test-results-changed", load)];
    return () => offs.forEach((p) => void p.then((f) => f()));
  }, [load]);

  const names = useMemo(() => new Map(model.nodes.map((n) => [n.id, n.name])), [model]);
  // A session with no task and no touched component is named by its branch
  // and start time; scryer never records the developer's prompts.
  const titles = useMemo(() => {
    const branchOf = (s: LandingSession) =>
      landing?.worktrees?.find((w) => w.path === s.worktree)?.branch ?? landing?.branch ?? "detached";
    return new Map(
      (landing?.sessions ?? []).map((s) => [s.session, s.title || `${branchOf(s)} · ${when(s.startedAt)}`]),
    );
  }, [landing]);

  // Open the claim's page and flash the claim once it renders.
  const openClaim = (c: LandingClaim) => {
    onSelectNode(c.nodeId);
    window.setTimeout(() => jumpTo(respElementId(c.respId)), 250);
  };

  const subtitle = error
    ? error
    : !landing
      ? "Loading…"
      : window_ === 0
        ? `${landing.sessions.length} sessions`
        : since === 0
          ? "First time here"
          : `Since you last looked · ${when(since)}`;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader title="Since you last looked" subtitle={subtitle} />
      <SpecialBody>
        <div className="mb-2 flex min-h-7 items-center">
          <button
            type="button"
            onClick={() => setShowAll((v) => !v)}
            className="ml-auto rounded-md px-2 py-1 text-xs text-[var(--text-muted)] hover:bg-[var(--surface-hover)]"
          >
            {showAll ? "Only since last look" : "Show earlier sessions"}
          </button>
        </div>
        {landing && landing.sessions.length === 0 && (landing.worktrees?.length ?? 0) < 2 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <History className="h-6 w-6 text-[var(--text-ghost)]" />
            <p className="text-sm text-[var(--text-muted)]">No agent sessions since you last looked.</p>
          </div>
        ) : (
          <>
            {landing && landing.shared.length > 0 && (
              <PageSection
                title="Touched by more than one session"
                hint="Components several sessions edited in this window — where their work may collide."
                count={landing.shared.length}
              >
                <ul className="flex flex-col gap-1">
                  {landing.shared.map((c) => (
                    <li key={c.nodeId} className="flex items-center gap-2 text-sm">
                      <Users className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
                      <button type="button" className="text-[var(--accent)] hover:underline" onClick={() => onSelectNode(c.nodeId)}>
                        {names.get(c.nodeId) ?? c.nodeId}
                      </button>
                      <span className="truncate text-xs text-[var(--text-muted)]">
                        {c.sessions.map((s) => titles.get(s) ?? s.slice(0, 8)).join(" · ")}
                      </span>
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}
            {landing && (landing.worktrees?.length ?? 0) > 1
              ? // Several checkouts: each worktree's sessions under its own
                // heading; a worktree with none is listed by its branch.
                landing.worktrees!.map((w) => {
                  const own = landing.sessions.filter((s) => s.worktree === w.path);
                  return (
                    <div key={w.path} className="mt-6">
                      <div className="flex items-baseline gap-2 border-b border-[var(--border)] pb-1">
                        <GitBranch className="h-3.5 w-3.5 shrink-0 self-center text-[var(--text-ghost)]" />
                        <span className="text-sm font-semibold text-[var(--text)]">{w.branch ?? "detached"}</span>
                        {w.current && <span className="text-xs text-[var(--accent)]">open here</span>}
                        <span className="ml-auto truncate font-mono text-xs text-[var(--text-ghost)]" title={w.path}>
                          {w.path}
                        </span>
                      </div>
                      {own.length === 0 ? (
                        <p className="py-2 text-xs text-[var(--text-muted)]">No sessions in this window.</p>
                      ) : (
                        own.map((s) => (
                          <SessionCard
                            key={s.session}
                            s={s}
                            title={titles.get(s.session) ?? s.title}
                            names={names}
                            branch={w.branch ?? "detached"}
                            onOpenClaim={openClaim}
                            onSelectNode={onSelectNode}
                          />
                        ))
                      )}
                    </div>
                  );
                })
              : landing?.sessions.map((s) => (
                  <SessionCard key={s.session} s={s} title={titles.get(s.session) ?? s.title} names={names} onOpenClaim={openClaim} onSelectNode={onSelectNode} />
                ))}
          </>
        )}
      </SpecialBody>
    </div>
  );
}

function SessionCard({
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
  /** The worktree's branch, shown beside the session's time span. */
  branch?: string;
  onOpenClaim: (c: LandingClaim) => void;
  onSelectNode: (id: string) => void;
}) {
  const groups: [string, LandingClaim[]][] = [
    ["Newly proven", s.newlyProven],
    ["Newly planned", s.newlyPlanned],
    ["Still planned", s.stillPlanned],
    ["Newly stale or failing", s.newlyStaleOrFailing],
  ];
  const quiet = groups.every(([, rows]) => rows.length === 0) && s.survivingMutants === 0 && s.newDarkFiles.length === 0;
  return (
    <PageSection
      title={title}
      right={
        <span className="text-xs text-[var(--text-muted)]">
          {branch && <span className="mr-2 font-mono text-[var(--text-tertiary)]">{branch}</span>}
          {when(s.startedAt)} – {when(s.endedAt)}
        </span>
      }
    >
      <div className="mb-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-[var(--text-muted)]">
        {groups.map(([label, rows]) => (
          <span key={label}>
            {label.toLowerCase()} <b className="text-[var(--text)]">{rows.length}</b>
          </span>
        ))}
        <span>
          surviving mutants <b className="text-[var(--text)]">{s.survivingMutants}</b>
        </span>
        <span>
          new dark files <b className="text-[var(--text)]">{s.newDarkFiles.length}</b>
        </span>
      </div>
      {quiet && <p className="text-sm text-[var(--text-muted)]">Nothing this session touched changed state.</p>}
      {groups
        .filter(([, rows]) => rows.length > 0)
        .map(([label, rows]) => (
          <div key={label} className="mb-2">
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
                    <span className={`shrink-0 text-xs ${TONE[c.state] ?? ""}`}>{c.state}</span>
                    <span className="min-w-0 flex-1 truncate text-[var(--text)]">{c.statement}</span>
                    <span className="shrink-0 text-xs text-[var(--text-ghost)]">{names.get(c.nodeId) ?? ""}</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
      {s.newDarkFiles.length > 0 && (
        <div className="mb-2 text-xs text-[var(--text-muted)]">
          Dark: {s.newDarkFiles.join(", ")}
        </div>
      )}
      {s.components.length > 0 && (
        <div className="flex flex-wrap gap-1 text-xs">
          {s.components.map((id) => (
            <button
              key={id}
              type="button"
              onClick={() => onSelectNode(id)}
              className="rounded bg-[var(--surface-inset)] px-1.5 py-0.5 text-[var(--text-muted)] hover:text-[var(--text)]"
            >
              {names.get(id) ?? id}
            </button>
          ))}
        </div>
      )}
    </PageSection>
  );
}
