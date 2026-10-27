/**
 * The comments rail beside a node's page: threads people and agents left on
 * this node, open ones first, each worked like an issue — replies, then an
 * explicit close. Closed threads fold to one line until opened.
 */

import { useState } from "react";
import { CircleCheck, CircleDot } from "lucide-react";
import type { CommentEvent, Thread, ThreadStatus } from "../../entities/model/comments";
import { BTN, BTN_GO, EYEBROW } from "../../shared/ui/tokens";

// The page's in-place field (Input's ghost variant): no box until hovered or
// focused, so the rail reads as page, not form.
const FIELD =
  "-mx-1 block w-[calc(100%+0.5rem)] resize-none rounded border-0 bg-transparent px-1 py-0.5 text-sm leading-snug text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] hover:bg-[color-mix(in_srgb,var(--text)_5%,transparent)] focus:bg-[var(--surface-field)] focus:hover:bg-[var(--surface-field)] focus:ring-1 focus:ring-[var(--accent)]";

const when = (ms: number) =>
  new Date(ms).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

function StatusChip({ status }: { status: ThreadStatus }) {
  return status === "open" ? (
    <span className="flex items-center gap-1 text-2xs font-medium text-emerald-700 dark:text-emerald-400">
      <CircleDot className="h-3 w-3" /> Open
    </span>
  ) : (
    <span className="flex items-center gap-1 text-2xs font-medium text-violet-700 dark:text-violet-400">
      <CircleCheck className="h-3 w-3" /> Addressed
    </span>
  );
}

function Event({ e }: { e: CommentEvent }) {
  return (
    <li className="flex flex-col gap-0.5">
      <span className="flex items-baseline gap-1.5 text-xs text-[var(--text-muted)]">
        <span className="font-medium text-[var(--text-secondary)]">{e.author === "human" ? "You" : "Agent"}</span>
        {e.status === "addressed" && <span className="whitespace-nowrap text-violet-700 dark:text-violet-400">closed</span>}
        {e.status === "open" && <span className="whitespace-nowrap text-emerald-700 dark:text-emerald-400">reopened</span>}
        <span className="ml-auto shrink-0 whitespace-nowrap font-mono">{when(e.at)}</span>
      </span>
      <span className="whitespace-pre-wrap break-words text-sm leading-snug text-[var(--text)]">{e.text}</span>
    </li>
  );
}

function ThreadCard({
  thread,
  onReply,
}: {
  thread: Thread;
  onReply: (text: string, status?: ThreadStatus) => Promise<void>;
}) {
  const closed = thread.status === "addressed";
  const [expanded, setExpanded] = useState(!closed);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const send = async (status?: ThreadStatus) => {
    if (!draft.trim()) return;
    setBusy(true);
    try {
      await onReply(draft, status);
      setDraft("");
    } finally {
      setBusy(false);
    }
  };
  const first = thread.events[0];
  if (!expanded) {
    return (
      <button
        type="button"
        onClick={() => setExpanded(true)}
        className="-mx-1 flex w-[calc(100%+0.5rem)] flex-col gap-0.5 rounded px-1 py-1 text-left hover:bg-[var(--surface-hover)]"
      >
        <StatusChip status={thread.status} />
        <span className="line-clamp-2 text-sm text-[var(--text-muted)]">{first?.text}</span>
      </button>
    );
  }
  return (
    <div className="flex flex-col gap-2 border-l-2 border-[var(--border)] pl-2.5">
      <div className="flex items-center gap-2">
        <StatusChip status={thread.status} />
        {closed && (
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className="ml-auto text-xs text-[var(--text-muted)] hover:text-[var(--text)]"
          >
            Fold
          </button>
        )}
      </div>
      <ul className="flex flex-col gap-2">
        {thread.events.map((e, i) => (
          <Event key={i} e={e} />
        ))}
      </ul>
      <textarea
        rows={2}
        value={draft}
        onChange={(ev) => setDraft(ev.target.value)}
        placeholder={closed ? "Why reopen?" : "Reply"}
        className={FIELD}
      />
      {draft.trim() && (
        <div className="flex justify-end gap-2">
          {closed ? (
            <button type="button" disabled={busy} onClick={() => void send("open")} className={BTN}>
              Reopen
            </button>
          ) : (
            <>
              <button type="button" disabled={busy} onClick={() => void send("addressed")} className={BTN}>
                Reply and close
              </button>
              <button type="button" disabled={busy} onClick={() => void send()} className={BTN_GO}>
                Reply
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

export function CommentsRail({
  threads,
  onOpen,
  onReply,
}: {
  /** This node's threads, open ones first. */
  threads: Thread[];
  onOpen: (text: string) => Promise<void>;
  onReply: (thread: string, text: string, status?: ThreadStatus) => Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const open = threads.filter((t) => t.status === "open").length;
  const start = async () => {
    if (!draft.trim()) return;
    setBusy(true);
    try {
      await onOpen(draft);
      setDraft("");
    } finally {
      setBusy(false);
    }
  };
  return (
    // Beside the article in a wide page, a full-width section under it in a
    // narrow one (the page is the size container).
    <aside
      data-comments-rail
      className="flex w-full shrink-0 flex-col gap-3 @3xl:sticky @3xl:top-0 @3xl:w-[280px] @3xl:self-start"
    >
      {/* The page sections' heading: eyebrow over a rule. */}
      <h2 className={`${EYEBROW} border-b border-[var(--border)] pb-[5px]`}>
        Comments
        {open > 0 && (
          <span className="ml-1.5 font-mono text-2xs font-normal normal-case tracking-normal text-[var(--text-ghost)]">
            {open} open
          </span>
        )}
      </h2>
      <div className="flex flex-col gap-1.5">
        <textarea
          rows={draft ? 3 : 2}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Ask an agent, or leave a decision for whoever works here next"
          className={FIELD}
        />
        {draft.trim() && (
          <button type="button" disabled={busy} onClick={() => void start()} className={`${BTN_GO} self-end`}>
            Comment
          </button>
        )}
      </div>
      {threads.map((t) => (
        <ThreadCard key={t.id} thread={t} onReply={(text, status) => onReply(t.id, text, status)} />
      ))}
    </aside>
  );
}
