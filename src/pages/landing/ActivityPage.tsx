/**
 * Activity — what happened to the model, the way a forge shows a
 * repository's commits. The agents working here now come first, each one
 * followable: the wiki and the map then go where it works. Then what is
 * uncommitted, then HEAD's commits newest first, each with what it changed
 * in the model: per node, added, modified or removed, its claims added,
 * reworded or removed under it, and its other changes in words. A node or
 * claim still in the model links to its page. Read-only.
 *
 * A code symbol is listed only when its claims changed; the rest of a
 * commit's symbol churn is a count, so the feed reads at the level of
 * components and claims.
 */

import { useMemo, useState } from "react";
import { Bot, Eye, GitBranch, GitCommitHorizontal } from "lucide-react";
import { jumpTo, PageSection, WordDiffText } from "../../shared/ui/pagekit";
import { respElementId } from "../../widgets/source-section/SourceSection";
import { SpecialBody, SpecialHeader } from "../../widgets/special-page-shell/shell";
import { ChangeGlyph, type ChangeKind } from "../../entities/model/diffkit";
import { stripMarkup } from "../../entities/model/markup";
import type { ScryModel } from "../../entities/model/viewmodel";
import { useAgentIntegration } from "../../features/agent-integration/useAgentIntegration";
import { RECENT_SECS, sessionTitle, useSessions, type LoggedSession } from "../../features/sessions/useSessions";
import {
  diffTally,
  useActivity,
  type Change,
  type DiffState,
  type ModelDiff,
  type NodeChange,
} from "../../features/activity/useActivity";
import type { ChangeBase } from "../../features/model-storage/useModelStorage";
import { FoldRow } from "./FoldRow";

const GLYPH: Record<Change, ChangeKind> = { added: "add", modified: "modified", removed: "delete" };

/** "just now", "12m ago", "3h ago", else the day. */
export function ago(at: number, now = Date.now() / 1000): string {
  const s = Math.max(0, now - at);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return new Date(at * 1000).toLocaleDateString([], { month: "short", day: "numeric" });
}

/** The nodes a diff lists, and how many code symbols it leaves to a count:
 *  a symbol is listed only when its claims changed. */
export function listedNodes(d: ModelDiff): { nodes: NodeChange[]; symbols: number } {
  const nodes = d.nodes.filter((n) => n.kind !== "symbol" || n.claims.length > 0);
  return { nodes, symbols: d.nodes.length - nodes.length };
}

export function ActivityPage({
  projectPath,
  model,
  changeBase,
  generation,
  following,
  onFollow,
  onSelectNode,
}: {
  projectPath: string | null;
  model: ScryModel;
  /** Where the checkout's changes start: marks the commits not on the
   *  branch it left. */
  changeBase: ChangeBase | null;
  /** Bumped when the tree changes: the feed is read again. */
  generation: number;
  /** The session the wiki and map follow, if any. */
  following: string | null;
  /** Follow a session, or stop with null. */
  onFollow: (session: string | null) => void;
  onSelectNode: (id: string) => void;
}) {
  const integration = useAgentIntegration(projectPath);
  const [recent] = useState(() => Math.floor(Date.now() / 1000) - RECENT_SECS);
  const sessions = useSessions(projectPath, recent);
  const activity = useActivity(projectPath, generation);
  const live = (sessions.data?.sessions ?? []).filter((s) => s.live);
  const branch = sessions.data?.branch ?? null;

  const nodes = useMemo(() => new Set(model.nodes.map((n) => n.id)), [model]);
  const claims = useMemo(() => {
    const at = new Map<string, string>();
    for (const n of model.nodes) for (const c of n.claims ?? []) at.set(c.id, n.id);
    return at;
  }, [model]);
  const links = {
    node: (id: string) => (nodes.has(id) ? () => onSelectNode(id) : undefined),
    claim: (id: string) => {
      const node = claims.get(id);
      return node
        ? () => {
            onSelectNode(node);
            window.setTimeout(() => jumpTo(respElementId(id)), 250);
          }
        : undefined;
    },
  };
  const ahead = changeBase?.against ? changeBase.ahead : 0;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Activity"
        subtitle={
          activity.error ??
          ([branch, live.length > 0 ? `${live.length} agent${live.length === 1 ? "" : "s"} working now` : null]
            .filter(Boolean)
            .join(" · ") ||
            "What changed in the model, commit by commit")
        }
      />
      <SpecialBody>
        <div className="[&>:first-child]:!mt-0">
          {live.length > 0 && (
            <PageSection title="Working now" count={live.length}>
              <ul className="flex flex-col">
                {live.map((s) => (
                  <LiveRow
                    key={s.session}
                    session={s}
                    title={sessionTitle(s, s.branch ?? branch ?? "detached")}
                    following={following === s.session}
                    onFollow={onFollow}
                  />
                ))}
              </ul>
            </PageSection>
          )}
          {integration.status && !integration.status.on && (
            <p className="mb-3 text-xs text-[var(--text-muted)]">
              Agent integration is off for this project, so agents working here are not shown. Turn it on in Settings,
              or run <code className="font-mono text-[var(--text-secondary)]">scryer-mcp setup on</code>.
            </p>
          )}
          <PageSection title="History">
            <ul className="flex flex-col">
              {changeBase?.dirty && (
                <Entry
                  icon={<span className="h-1.5 w-1.5 rounded-full bg-amber-600 dark:bg-amber-400" />}
                  title="Uncommitted changes"
                  meta="working tree"
                  state={activity.uncommitted}
                  links={links}
                  defaultOpen
                />
              )}
              {(activity.commits ?? []).map((c, i) => (
                <Entry
                  key={c.sha}
                  icon={<GitCommitHorizontal className="h-3.5 w-3.5 text-[var(--text-ghost)]" />}
                  title={c.subject}
                  meta={`${c.sha.slice(0, 7)} · ${c.author} · ${ago(c.at)}`}
                  tag={i < ahead ? `not in ${changeBase?.against}` : undefined}
                  state={activity.diffs[c.sha]}
                  links={links}
                />
              ))}
            </ul>
            {activity.commits === null && !activity.error && (
              <p className="py-1 text-sm text-[var(--text-muted)]">Loading…</p>
            )}
            {activity.commits?.length === 0 && (
              <p className="py-1 text-sm text-[var(--text-muted)]">No commits yet.</p>
            )}
            {activity.more && (activity.commits?.length ?? 0) > 0 && (
              <button
                type="button"
                onClick={activity.showMore}
                className="mt-1 px-1 text-xs text-[var(--text-muted)] hover:underline"
              >
                Older commits
              </button>
            )}
          </PageSection>
        </div>
      </SpecialBody>
    </div>
  );
}

export function LiveRow({
  session,
  title,
  following,
  onFollow,
}: {
  session: LoggedSession;
  title: string;
  following: boolean;
  onFollow: (session: string | null) => void;
}) {
  return (
    <li className="flex min-w-0 items-center gap-2 px-1 py-1 text-sm">
      <Bot className="h-3.5 w-3.5 shrink-0 animate-pulse text-violet-600 dark:text-violet-400" />
      <span className="min-w-0 flex-1 truncate text-[var(--text)]">{title}</span>
      {session.branch && (
        <span className="flex shrink-0 items-center gap-1 font-mono text-xs text-[var(--text-muted)]">
          <GitBranch className="h-3 w-3" />
          {session.branch}
        </span>
      )}
      <button
        type="button"
        aria-pressed={following}
        onClick={() => onFollow(following ? null : session.session)}
        title={following ? "Stop following this session" : "Follow this session: the wiki and map go where it works"}
        className={`flex shrink-0 items-center gap-1 whitespace-nowrap rounded-md px-2 py-0.5 text-xs font-medium ${
          following
            ? "bg-violet-600 text-white dark:bg-violet-500"
            : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
        }`}
      >
        <Eye className="h-3.5 w-3.5" />
        {following ? "Following" : "Follow"}
      </button>
    </li>
  );
}

interface Links {
  node: (id: string) => (() => void) | undefined;
  claim: (id: string) => (() => void) | undefined;
}

/** "+3 ~1 −2" for the claims, then the nodes changed otherwise. */
function Tally({ diff }: { diff: ModelDiff }) {
  const t = diffTally(diff);
  const { symbols } = listedNodes(diff);
  if (t.added + t.reworded + t.removed + t.nodes === 0)
    return <span className="text-xs text-[var(--text-ghost)]">{symbols > 0 ? "code only" : "no model change"}</span>;
  return (
    <span className="flex shrink-0 items-baseline gap-1.5 font-mono text-xs tabular-nums">
      {t.added > 0 && <span className="text-emerald-700 dark:text-emerald-400">+{t.added}</span>}
      {t.reworded > 0 && <span className="text-amber-700 dark:text-amber-400">~{t.reworded}</span>}
      {t.removed > 0 && <span className="text-red-700 dark:text-red-400">−{t.removed}</span>}
      {t.nodes > 0 && (
        <span className="font-sans text-[var(--text-muted)]">
          {t.nodes} node{t.nodes === 1 ? "" : "s"}
        </span>
      )}
    </span>
  );
}

function Entry({
  icon,
  title,
  meta,
  tag,
  state,
  links,
  defaultOpen = false,
}: {
  icon: React.ReactNode;
  title: string;
  meta: string;
  /** Says the commit is not on the branch the checkout left. */
  tag?: string;
  state: DiffState;
  links: Links;
  defaultOpen?: boolean;
}) {
  const diff = state && state !== "loading" && "diff" in state ? state.diff : null;
  const empty = diff !== null && diff.nodes.length === 0;
  return (
    <FoldRow
      defaultOpen={defaultOpen}
      summary={
        <span className="flex min-w-0 flex-1 items-baseline gap-2">
          <span className="flex w-3.5 shrink-0 items-center justify-center self-center">{icon}</span>
          <span className="min-w-0 flex-1 truncate text-[var(--text)]">{title}</span>
          {tag && <span className="shrink-0 rounded bg-[var(--surface-tint)] px-1 text-2xs text-[var(--text-muted)]">{tag}</span>}
          {state === "loading" || state === undefined ? (
            <span className="shrink-0 text-xs text-[var(--text-ghost)]">reading…</span>
          ) : "error" in state ? (
            <span className="shrink-0 text-xs text-[var(--text-ghost)]" title={state.error}>
              unreadable
            </span>
          ) : (
            <Tally diff={state.diff} />
          )}
          <span className="hidden w-48 shrink-0 truncate text-right font-mono text-2xs text-[var(--text-ghost)] sm:block">
            {meta}
          </span>
        </span>
      }
    >
      {diff && !empty ? <DiffView diff={diff} links={links} /> : null}
    </FoldRow>
  );
}

/** A diff, node by node: its change, where it sits, its other changes in
 *  words, then its claims. */
export function DiffView({ diff, links }: { diff: ModelDiff; links: Links }) {
  const { nodes, symbols } = listedNodes(diff);
  return (
    <div className="flex flex-col gap-2">
      {nodes.map((n) => {
        const open = n.change === "removed" ? undefined : links.node(n.id);
        return (
          <div key={n.id} data-node-change={n.change} className="flex flex-col">
            <div className="grid grid-cols-[18px_1fr] items-baseline text-sm">
              <ChangeGlyph kind={GLYPH[n.change]} />
              <span className="min-w-0 truncate">
                {n.path.length > 0 && <span className="text-[var(--text-ghost)]">{n.path.join(" / ")} / </span>}
                {open ? (
                  <button type="button" onClick={open} className="text-blue-700 hover:underline dark:text-blue-400">
                    {n.name}
                  </button>
                ) : (
                  <span className={n.change === "removed" ? "text-[var(--text-muted)] line-through" : "text-[var(--text)]"}>
                    {n.name}
                  </span>
                )}
                {n.notes && n.notes.length > 0 && (
                  <span className="text-xs text-[var(--text-muted)]"> — {n.notes.join(" · ")}</span>
                )}
              </span>
            </div>
            {n.claims.length > 0 && (
              <ul className="ml-[18px] flex flex-col">
                {n.claims.map((c) => {
                  const go = c.change === "removed" ? undefined : links.claim(c.id);
                  const text =
                    c.change === "modified" && c.was ? (
                      <WordDiffText from={stripMarkup(c.was)} to={stripMarkup(c.statement)} />
                    ) : (
                      stripMarkup(c.statement)
                    );
                  return (
                    <li key={`${c.change}:${c.id}`} className="grid grid-cols-[18px_1fr] items-baseline font-mono text-sm">
                      <ChangeGlyph kind={GLYPH[c.change]} />
                      {go ? (
                        <button type="button" onClick={go} className="text-left text-[var(--text)] hover:underline">
                          {text}
                        </button>
                      ) : (
                        <span className={c.change === "removed" ? "text-[var(--text-muted)] line-through" : "text-[var(--text)]"}>
                          {text}
                        </span>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        );
      })}
      {symbols > 0 && (
        <p className="text-xs text-[var(--text-muted)]">
          and {symbols} code symbol{symbols === 1 ? "" : "s"} changed without a claim
        </p>
      )}
    </div>
  );
}
