/**
 * Session — the log of one agent session, read at a glance while the agent
 * works in the CLI: the plan entries it left unbuilt, the files it edited and
 * the claims those edits reached, and the plan elements it wrote. Fed by
 * `useSessionLog`; read-only — the developer acts in the conversation or on
 * the claim's own page.
 */

import { useMemo } from "react";
import { History, Radio, TriangleAlert } from "lucide-react";
import type { SessionLog } from "../../features/session-log/useSessionLog";
import { claimIndex, resolveKey, type ClaimHost } from "../../features/session-log/session";
import { ANCHOR_CALM, StatementText } from "../../features/markup/markup";
import { jumpTo, PageSection } from "../../shared/ui/pagekit";
import { PILL_BASE } from "../../shared/ui/statusColors";
import { Select } from "../../shared/ui/Select";
import { respElementId } from "../../widgets/source-section/SourceSection";
import type { ScryModel } from "../../entities/model/viewmodel";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";

const when = (at: number) =>
  new Date(at * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

export function SessionPage({
  model,
  log,
  onSelectNode,
  onSelectGroup,
}: {
  model: ScryModel;
  log: SessionLog;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
}) {
  const { sessions, selectedId, select, view, live } = log;
  const claims = useMemo(() => claimIndex(model), [model]);

  const openHost = (kind: "node" | "group", id: string) => (kind === "group" ? onSelectGroup(id) : onSelectNode(id));
  // Open the claim's page and flash the claim once it renders.
  const openClaim = (respId: string) => {
    const c = claims.get(respId);
    if (!c) return;
    openHost(c.hostKind, c.hostId);
    window.setTimeout(() => jumpTo(respElementId(respId)), 250);
  };

  const sessionOptions = useMemo(
    () =>
      sessions.map((s) => ({
        value: s.session,
        label: `${when(s.updatedAt)} · ${s.session.slice(0, 8)}`,
      })),
    [sessions],
  );

  const subtitle = !view
    ? sessions.length === 0
      ? "No agent sessions yet."
      : "Loading…"
    : `Started ${when(view.startedAt)} · ${plural(view.touched.length, "file")} touched · ${plural(view.modelEdits.length, "model edit")}`;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader title="Session" subtitle={subtitle} />
      <SpecialBody>
        {sessions.length === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <History className="h-6 w-6 text-[var(--text-ghost)]" />
            <p className="text-sm text-[var(--text-muted)]">
              Sessions appear here when an agent works in this project with scryer's hooks installed.
            </p>
          </div>
        ) : (
          <>
            <div className="mb-2 flex min-h-7 items-center gap-3">
              {live && (
                <span
                  className={`${PILL_BASE} bg-emerald-500/10 text-emerald-700 ring-emerald-500/25 dark:bg-emerald-400/10 dark:text-emerald-300 dark:ring-emerald-400/25`}
                  title="An agent session was written to in the last ten minutes"
                >
                  <Radio className="h-3 w-3 animate-pulse" />
                  Agent active
                </span>
              )}
              {sessions.length > 1 && (
                <div className="ml-auto flex w-80" title="Show another session">
                  <Select
                    options={sessionOptions}
                    value={selectedId ?? ""}
                    // Picking the newest follows whatever is newest next.
                    onChange={(v) => select(v === sessions[0]?.session ? null : v)}
                    active={selectedId !== sessions[0]?.session}
                  />
                </div>
              )}
            </div>

            {view && (view.unfolded?.length ?? 0) > 0 && (
              <PageSection
                title="Planned, not built"
                hint="Model entries the agent planned this session and never folded: work it left unfinished."
                count={view.unfolded!.length}
              >
                <ul className="flex flex-col gap-1 rounded-md border border-orange-500/30 bg-orange-500/5 px-3 py-2 dark:border-orange-400/30 dark:bg-orange-400/5">
                  {view.unfolded!.map((u) => (
                    <li key={u.key} className="flex items-center gap-2 text-sm text-orange-800 dark:text-orange-300">
                      <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
                      <span className="truncate" title={u.key}>
                        {u.label}
                        {u.note ? <span className="text-[var(--text-muted)]"> — {u.note}</span> : " — no note"}
                      </span>
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}

            {view && view.touched.length > 0 && (
              <PageSection
                title="Files touched"
                hint="Each file the agent edited and the claims anchored in it."
                count={view.touched.length}
              >
                <ul className="flex flex-col">
                  {view.touched.map((t) => (
                    <li key={t.file} className="border-b border-[var(--border-subtle)] py-2 last:border-b-0">
                      <div className="truncate font-mono text-xs text-[var(--text-secondary)]" title={t.file}>
                        {t.file}
                      </div>
                      {t.claims.length > 0 ? (
                        <ul className="mt-1 flex flex-col gap-0.5 pl-3">
                          {t.claims.map(([id, statement]) => (
                            <ClaimLine key={id} id={id} fallback={statement} claims={claims} onOpen={openClaim} />
                          ))}
                        </ul>
                      ) : (
                        <div className="mt-0.5 pl-3 text-xs italic text-[var(--text-ghost)]">no claim anchored here</div>
                      )}
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}

            {view && view.modelEdits.length > 0 && (
              <PageSection
                title="Model edits"
                hint="Plan elements the agent wrote this session."
                count={view.modelEdits.length}
              >
                <ul className="flex flex-col">
                  {view.modelEdits.map((key) => {
                    const r = resolveKey(key, model, claims);
                    const go = r.respId
                      ? () => openClaim(r.respId!)
                      : r.hostKind && r.hostId
                        ? () => openHost(r.hostKind!, r.hostId!)
                        : undefined;
                    return (
                      <li key={key} className="flex items-baseline gap-2 border-b border-[var(--border-subtle)] py-1.5 last:border-b-0">
                        <span className="w-12 shrink-0 font-mono text-2xs text-[var(--text-ghost)]">{r.kind}</span>
                        {go ? (
                          <button
                            type="button"
                            onClick={go}
                            title={key}
                            className="min-w-0 truncate text-left text-sm text-[var(--text-secondary)] hover:text-[var(--text)] hover:underline"
                          >
                            {r.kind === "resp" ? <StatementText text={r.label} anchor={ANCHOR_CALM} /> : r.label}
                          </button>
                        ) : (
                          <span className="min-w-0 truncate font-mono text-xs text-[var(--text-muted)]">{r.label}</span>
                        )}
                        {r.hostName && <span className="shrink-0 text-xs text-[var(--text-muted)]">on {r.hostName}</span>}
                      </li>
                    );
                  })}
                </ul>
              </PageSection>
            )}
          </>
        )}
      </SpecialBody>
    </div>
  );
}

/** A claim by id: its statement (opens it on its page) and host, or the bare
 *  id when the model no longer holds it. */
function ClaimLine({
  id,
  fallback,
  claims,
  onOpen,
}: {
  id: string;
  fallback?: string;
  claims: Map<string, ClaimHost>;
  onOpen: (respId: string) => void;
}) {
  const c = claims.get(id);
  return (
    <li className="flex min-w-0 items-baseline gap-1.5 text-xs">
      {c ? (
        <>
          <button
            type="button"
            onClick={() => onOpen(id)}
            title="Open on its page"
            className="min-w-0 truncate text-left font-mono text-[var(--text-secondary)] hover:text-[var(--text)] hover:underline"
          >
            <StatementText text={c.statement} anchor={ANCHOR_CALM} />
          </button>
          <span className="shrink-0 text-[var(--text-muted)]">on {c.hostName || "Untitled"}</span>
        </>
      ) : (
        <span className="min-w-0 truncate font-mono text-[var(--text-muted)]" title={id}>
          {fallback || id}
        </span>
      )}
    </li>
  );
}
