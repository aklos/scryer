/**
 * The comments rail beside a node's page: threads people and agents left on
 * this node, open ones first, each worked like an issue — replies, then an
 * explicit close. Closed threads fold to one line until opened.
 */

import { useState } from "react";
import { CircleCheck, CircleDot, MessageSquare } from "lucide-react";
import type { CommentEvent, Thread, ThreadStatus } from "../../entities/model/comments";
import { BTN, BTN_GO } from "../../shared/ui/tokens";

const FIELD =
  "w-full resize-y rounded border border-[var(--border)] bg-[var(--surface-field)] px-2 py-1.5 text-xs leading-snug text-[var(--text)] placeholder:text-[var(--text-ghost)] focus:border-[var(--accent)] focus:outline-none";

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
      <span className="flex items-baseline gap-1.5 text-2xs text-[var(--text-muted)]">
        <span className="font-medium text-[var(--text-secondary)]">{e.author === "human" ? "You" : "Agent"}</span>
        {e.status === "addressed" && <span className="whitespace-nowrap text-violet-700 dark:text-violet-400">closed</span>}
        {e.status === "open" && <span className="whitespace-nowrap text-emerald-700 dark:text-emerald-400">reopened</span>}
        <span className="ml-auto shrink-0 whitespace-nowrap font-mono">{when(e.at)}</span>
      </span>
      <span className="whitespace-pre-wrap break-words text-xs leading-snug text-[var(--text)]">{e.text}</span>
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
        className="flex w-full flex-col gap-0.5 rounded-md border border-[var(--border)] px-2.5 py-2 text-left hover:bg-[var(--surface-hover)]"
      >
        <StatusChip status={thread.status} />
        <span className="line-clamp-2 text-xs text-[var(--text-muted)]">{first?.text}</span>
      </button>
    );
  }
  return (
    <div className="flex flex-col gap-2 rounded-md border border-[var(--border)] px-2.5 py-2">
      <div className="flex items-center gap-2">
        <StatusChip status={thread.status} />
        {closed && (
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className="ml-auto text-2xs text-[var(--text-muted)] hover:text-[var(--text)]"
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
      <div className="flex justify-end gap-2">
        {closed ? (
          <button type="button" disabled={busy || !draft.trim()} onClick={() => void send("open")} className={BTN}>
            Reopen
          </button>
        ) : (
          <>
            <button type="button" disabled={busy || !draft.trim()} onClick={() => void send("addressed")} className={BTN}>
              Reply and close
            </button>
            <button type="button" disabled={busy || !draft.trim()} onClick={() => void send()} className={BTN_GO}>
              Reply
            </button>
          </>
        )}
      </div>
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
    <aside data-comments-rail className="sticky top-0 flex w-[280px] shrink-0 flex-col gap-3 self-start">
      <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
        <MessageSquare className="h-3.5 w-3.5" />
        Comments
        {open > 0 && <span className="font-mono text-[var(--text-muted)]">{open} open</span>}
      </div>
      <div className="flex flex-col gap-1.5">
        <textarea
          rows={3}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Ask an agent something about this, or leave a decision for whoever works here next"
          className={FIELD}
        />
        <button
          type="button"
          disabled={busy || !draft.trim()}
          onClick={() => void start()}
          className={`${BTN_GO} self-end`}
        >
          Comment
        </button>
      </div>
      {threads.map((t) => (
        <ThreadCard key={t.id} thread={t} onReply={(text, status) => onReply(t.id, text, status)} />
      ))}
    </aside>
  );
}
