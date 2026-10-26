/**
 * Landing — what changed since you last looked. One card per agent session
 * active since the project was last opened, newest first, titled by its task
 * (else the first component it touched, else its branch and start time):
 * the claims its edits newly proved, newly planned or left planned, the ones
 * now stale or failing, surviving mutants and new dark files in what it
 * touched. Beside them, the components more than one session touched. Above
 * them all, where things stand now: the claim counts, and the failing and
 * planned claims by name, so the page answers even when nothing moved. Rows
 * open the claim's node page and flash the claim. Computed at read time from
 * the session logs and the derived status (`get_landing`); read-only. With
 * several git worktrees, sessions are grouped by the worktree they worked
 * in, each read against that checkout's own tree, and the shared-component
 * count spans them all; a worktree with no session in the window is left out.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { GitBranch, History, Users } from "lucide-react";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { respElementId } from "../../widgets/source-section/SourceSection";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";
import { CLAIM_LABEL, CLAIM_TONE } from "../../entities/model/claimState";
import type { ScryModel } from "../../entities/model/viewmodel";
import type { DerivedStatus } from "../../entities/model/derived";
import { useAgentIntegration } from "../../features/agent-integration/useAgentIntegration";

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
  since,
  model,
  status,
  onSelectNode,
}: {
  projectPath: string | null;
  /** When the project was last opened before now (unix seconds). */
  since: number;
  model: ScryModel;
  /** The derived status of every claim, for where things stand now. */
  status: DerivedStatus | null;
  onSelectNode: (id: string) => void;
}) {
  const integration = useAgentIntegration(projectPath);
  const [landing, setLanding] = useState<Landing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
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
        <div className="[&>:first-child]:!mt-0 [&_section]:mt-2">
          <Standing status={status} model={model} onOpenClaim={openClaim} />
          {integration.status && !integration.status.on && (
            <p className="mt-3 text-xs text-[var(--text-muted)]">
              Agent integration is off for this project, so no sessions are recorded. Turn it on in Settings, or run{" "}
              <code className="font-mono text-[var(--text-secondary)]">scryer-mcp setup on</code>.
            </p>
          )}
          {landing && landing.sessions.length === 0 ? (
            <div className="flex items-center gap-2 py-6">
              <History className="h-4 w-4 text-[var(--text-ghost)]" />
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
                  // heading; a worktree with none in the window is left out.
                  landing.worktrees!.map((w) => {
                    const own = landing.sessions.filter((s) => s.worktree === w.path);
                    if (own.length === 0) return null;
                    return (
                      <div key={w.path} className="mt-4">
                        <div className="flex items-baseline gap-2 border-b border-[var(--border)] pb-1">
                          <GitBranch className="h-3.5 w-3.5 shrink-0 self-center text-[var(--text-ghost)]" />
                          <span className="text-sm font-semibold text-[var(--text)]">{w.branch ?? "detached"}</span>
                          {w.current && <span className="text-xs text-[var(--accent)]">open here</span>}
                          <span className="ml-auto truncate font-mono text-xs text-[var(--text-ghost)]" title={w.path}>
                            {w.path}
                          </span>
                        </div>
                        {own.map((s) => (
                          <SessionCard
                            key={s.session}
                            s={s}
                            title={titles.get(s.session) ?? s.title}
                            names={names}
                            branch={w.branch ?? "detached"}
                            onOpenClaim={openClaim}
                            onSelectNode={onSelectNode}
                          />
                        ))}
                      </div>
                    );
                  })
                : landing?.sessions.map((s) => (
                    <SessionCard key={s.session} s={s} title={titles.get(s.session) ?? s.title} names={names} onOpenClaim={openClaim} onSelectNode={onSelectNode} />
                  ))}
            </>
          )}
        </div>
      </SpecialBody>
    </div>
  );
}

/** Where things stand now, whatever moved: every claim's count by state, a
 *  line on what stale means when anything is, and the failing and planned
 *  claims by name — the work that is still open. */
function Standing({
  status,
  model,
  onOpenClaim,
}: {
  status: DerivedStatus | null;
  model: ScryModel;
  onOpenClaim: (c: Pick<LandingClaim, "respId" | "nodeId">) => void;
}) {
  const statements = useMemo(() => {
    const out = new Map<string, string>();
    for (const n of model.nodes) for (const r of n.responsibilities ?? []) out.set(r.id, r.statement);
    return out;
  }, [model]);
  if (!status) return null;
  const t = status.totals;
  const open = (["failing", "planned"] as const).map(
    (state) => [state, status.claims.filter((c) => c.state === state)] as const,
  );
  const cells: [ClaimState, number][] = [
    ["proven", t.proven],
    ["stale", t.stale],
    ["failing", t.failing],
    ["planned", t.planned],
  ];
  return (
    <PageSection title="Where things stand" count={t.claims}>
      <div className="mb-2 flex flex-wrap gap-x-6 gap-y-1">
        {cells.map(([state, n]) => (
          <span key={state} className="flex items-baseline gap-1.5">
            <b className={`font-mono text-lg ${n > 0 ? TONE[state] : "text-[var(--text-ghost)]"}`}>{n}</b>
            <span className="text-xs text-[var(--text-muted)]">{CLAIM_LABEL[state]}</span>
          </span>
        ))}
      </div>
      {t.stale > 0 && (
        <p className="mb-2 text-xs text-[var(--text-muted)]">
          Stale: the test or the code it exercises changed since its last recorded run. Run the tests and ingest
          the report to refresh.
        </p>
      )}
      {open
        .filter(([, rows]) => rows.length > 0)
        .map(([state, rows]) => (
          <div key={state} className="mb-2">
            <div className="text-[11px] font-medium uppercase tracking-wide text-[var(--text-tertiary)]">{CLAIM_LABEL[state]}</div>
            <ul className="flex flex-col">
              {rows.map((c) => (
                <li key={c.respId}>
                  <button
                    type="button"
                    onClick={() => onOpenClaim(c)}
                    className="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left text-sm hover:bg-[var(--surface-hover)]"
                  >
                    <span className="min-w-0 flex-1 truncate text-[var(--text)]">{statements.get(c.respId) ?? c.respId}</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
    </PageSection>
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
    ["New todo", s.newlyPlanned],
    ["Still todo", s.stillPlanned],
    ["Newly stale or failing", s.newlyStaleOrFailing],
  ];
  // Only the counts that moved; a block where nothing did says so once.
  const all: [string, number][] = [
    ...groups.map(([label, rows]): [string, number] => [label.toLowerCase(), rows.length]),
    ["surviving mutants", s.survivingMutants],
    ["new dark files", s.newDarkFiles.length],
  ];
  const counts = all.filter(([, n]) => n > 0);
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
      {counts.length === 0 ? (
        <p className="mb-1 text-sm text-[var(--text-muted)]">Nothing changed state.</p>
      ) : (
        <div className="mb-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-[var(--text-muted)]">
          {counts.map(([label, n]) => (
            <span key={label}>
              {label} <b className="text-[var(--text)]">{n}</b>
            </span>
          ))}
        </div>
      )}
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
      {s.newDarkFiles.length > 0 && (
        <div className="mb-2 text-xs text-[var(--text-muted)]">
          Dark: {s.newDarkFiles.join(", ")}
        </div>
      )}
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
    </PageSection>
  );
}
