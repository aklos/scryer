/**
 * The Inbox — a single live stream of cards, each one item that needs the
 * developer's verdict, with the claim, the evidence, and the actions that
 * resolve it. Ordered by risk tier then recency (`src/inbox.ts`); fed by
 * `useInbox`. Keyboard: j/k move focus, a/r/e adopt/reject/reword the focused
 * card when those actions exist, Enter opens the card's node page.
 *
 * Not an agent chat: the developer resolves items here, the agent sees the
 * outcome through the model on its next read.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  CheckCheck,
  Crosshair,
  FlaskConical,
  Inbox as InboxIcon,
  Radio,
  RefreshCw,
  X,
} from "lucide-react";
import { ConfirmPopover } from "../ConfirmPopover";
import type { Editor } from "../editor";
import type { Inbox } from "../hooks/useInbox";
import { filterByChange, inboxKeyAction, type InboxAction, type InboxCard, type InboxTier } from "../inbox";
import { ANCHOR_CALM, serializeEars, StatementText, stripMarkup } from "../markup";
import { BTN, BTN_DANGER, BTN_GO, jumpTo, LINK, WordDiffText } from "../pagekit";
import { respElementId } from "../SourceSection";
import { PILL_BASE } from "../statusColors";
import { Select } from "../ui/Select";
import type { ScryModel } from "../viewmodel";
import { RewordEditor } from "./NeedsReviewPage";
import { SpecialBody, SpecialHeader } from "./shell";

/** Tier tag hue + label. Red for what is wrong now (a broken contract, a test
 *  that lied, a failing run), violet for the agent's proposals, orange for
 *  drift, blue for a session's open question, neutral for the informational
 *  refusal. */
const TIER_META: Record<InboxTier, { label: string; cls: string }> = {
  contract: {
    label: "Contract",
    cls: "bg-red-500/10 text-red-700 ring-red-500/25 dark:bg-red-400/10 dark:text-red-300 dark:ring-red-400/25",
  },
  concern: {
    label: "Concern",
    cls: "bg-fuchsia-500/10 text-fuchsia-700 ring-fuchsia-500/25 dark:bg-fuchsia-400/10 dark:text-fuchsia-300 dark:ring-fuchsia-400/25",
  },
  survivor: {
    label: "Survivor",
    cls: "bg-red-500/10 text-red-700 ring-red-500/25 dark:bg-red-400/10 dark:text-red-300 dark:ring-red-400/25",
  },
  amendment: {
    label: "Reworded after sign-off",
    cls: "bg-violet-500/10 text-violet-700 ring-violet-500/25 dark:bg-violet-400/10 dark:text-violet-300 dark:ring-violet-400/25",
  },
  vagrant: {
    label: "Undescribed",
    cls: "bg-orange-500/10 text-orange-700 ring-orange-500/25 dark:bg-orange-400/10 dark:text-orange-300 dark:ring-orange-400/25",
  },
  stale: {
    label: "Stale",
    cls: "bg-orange-500/10 text-orange-700 ring-orange-500/25 dark:bg-orange-400/10 dark:text-orange-300 dark:ring-orange-400/25",
  },
  failing: {
    label: "Failing",
    cls: "bg-red-500/10 text-red-700 ring-red-500/25 dark:bg-red-400/10 dark:text-red-300 dark:ring-red-400/25",
  },
  refused: {
    label: "Fold refused",
    cls: "bg-[var(--surface-hover)] text-[var(--text-tertiary)] ring-[var(--border-strong)]",
  },
  "close-gate": {
    label: "Close gate",
    cls: "bg-blue-500/10 text-blue-700 ring-blue-500/25 dark:bg-blue-400/10 dark:text-blue-300 dark:ring-blue-400/25",
  },
};

/** Kinds whose `title` says more than the tier pill already does — a survivor's
 *  count, a refusal's reason, the session's touch. Every other title restates
 *  the pill and is dropped. */
const TITLED_KINDS = new Set<InboxCard["kind"]>(["survivor", "refused", "close-gate"]);

/** The pill's text: the tier, narrowed by kind where the tier is ambiguous. */
function pillLabel(card: InboxCard): string {
  if (card.kind === "addition") return "Added after sign-off";
  return TIER_META[card.tier].label;
}

function isEditableTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable;
}

function actionClass(kind: InboxAction["kind"]): string {
  switch (kind) {
    case "adopt":
    case "accept-proposal":
      return BTN_GO;
    case "reject":
    case "drop":
      return BTN_DANGER;
    default:
      return BTN;
  }
}

/** Actions that write the model need the editor (absent while an agent owns
 *  the file); the rest are local or navigational and always available. */
const NEEDS_EDITOR = new Set<InboxAction["kind"]>([
  "adopt",
  "reject",
  "reword",
  "accept-proposal",
  "reimplement",
  "drop",
]);

export function InboxPage({
  model,
  inbox,
  editor,
  onSelectNode,
  onSelectGroup,
}: {
  model: ScryModel;
  inbox: Inbox;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
}) {
  const { cards: allCards, pinnedChange, setPinnedChange, markSeen, dismiss, live } = inbox;
  const cards = useMemo(() => filterByChange(allCards, pinnedChange), [allCards, pinnedChange]);
  const registry = model.changes ?? [];
  const rationaleOf = useMemo(() => new Map(registry.map((c) => [c.id, c.rationale] as const)), [registry]);

  // Being on the page reads the stream: everything listed counts as seen.
  useEffect(() => {
    for (const c of cards) markSeen(c.id);
  }, [cards, markSeen]);

  // Keyboard focus — an index into the filtered list, clamped as cards leave.
  const [focus, setFocus] = useState(0);
  useEffect(() => {
    if (focus >= cards.length) setFocus(Math.max(0, cards.length - 1));
  }, [cards.length, focus]);
  const [rewording, setRewording] = useState<string | null>(null);
  const [confirmDrop, setConfirmDrop] = useState<{ rect: DOMRect; run: () => void } | null>(null);
  const rowRefs = useRef(new Map<string, HTMLLIElement>());

  const openCard = useCallback(
    (card: InboxCard) => {
      if (card.hostKind === "group") onSelectGroup(card.nodeId);
      else onSelectNode(card.nodeId);
      if (card.respId) {
        const id = card.respId;
        window.setTimeout(() => jumpTo(respElementId(id)), 250);
      }
    },
    [onSelectNode, onSelectGroup],
  );

  const runAction = useCallback(
    (card: InboxCard, action: InboxAction, anchor?: DOMRect) => {
      if (NEEDS_EDITOR.has(action.kind) && !editor) return;
      switch (action.kind) {
        case "adopt":
          if (card.kind === "vagrant-property" && card.propLabel) editor!.adoptProperty(card.nodeId, card.propLabel);
          else if (card.respId) editor!.adoptResponsibility(card.respId);
          return;
        case "reject":
          if (card.kind === "vagrant-property" && card.propLabel) editor!.rejectProperty(card.nodeId, card.propLabel);
          else if (card.respId) editor!.rejectResponsibility(card.respId);
          return;
        case "reword":
          setRewording((cur) => (cur === card.id ? null : card.id));
          return;
        case "accept-proposal":
          if (card.respId && card.after) editor!.rewordResponsibility(card.respId, card.after);
          return;
        case "reimplement":
          if (card.respId) editor!.reimplementResponsibility(card.respId);
          return;
        case "drop":
          if (card.respId) {
            const respId = card.respId;
            if (anchor) setConfirmDrop({ rect: anchor, run: () => editor!.dropResponsibility(respId) });
            else editor!.dropResponsibility(respId);
          }
          return;
        case "approve":
        case "dismiss":
        case "holds":
          dismiss(card.id);
          return;
        case "open":
        case "open-test":
        case "flag":
          openCard(card);
          return;
      }
    },
    [editor, dismiss, openCard],
  );

  // j/k move, a/r/e act, Enter opens — skipped while typing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || isEditableTarget(e.target)) return;
      const action = inboxKeyAction(e.key, cards, focus);
      if (!action) return;
      e.preventDefault();
      switch (action.type) {
        case "focus":
          setFocus(action.index);
          return;
        case "open":
          openCard(action.card);
          return;
        case "run":
          runAction(action.card, action.action);
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cards, focus, openCard, runAction]);

  // Keep the focused card in view as j/k walk the list.
  useEffect(() => {
    const card = cards[focus];
    if (!card) return;
    rowRefs.current.get(card.id)?.scrollIntoView({ block: "nearest" });
  }, [cards, focus]);

  const subtitle =
    cards.length === 0
      ? "Nothing needs your verdict."
      : `${cards.length} item${cards.length === 1 ? "" : "s"} awaiting your verdict${pinnedChange ? ` in ${pinnedChange}` : ""}`;
  const changeOptions = useMemo(
    () => [{ value: "", label: "All changes" }, ...registry.map((c) => ({ value: c.id, label: `${c.id} · ${c.rationale}` }))],
    [registry],
  );

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader title="Inbox" subtitle={subtitle} />
      <SpecialBody>
        <div className="mb-4 flex min-h-7 items-center gap-3">
          {live && (
            <span
              className={`${PILL_BASE} bg-emerald-500/10 text-emerald-700 ring-emerald-500/25 dark:bg-emerald-400/10 dark:text-emerald-300 dark:ring-emerald-400/25`}
              title="An agent session touched code in the last ten minutes"
            >
              <Radio className="h-3 w-3 animate-pulse" />
              Agent active
            </span>
          )}
          {registry.length > 0 && (
            <div className="ml-auto flex w-72" title="Show only one open change">
              <Select
                options={changeOptions}
                value={pinnedChange ?? ""}
                onChange={(v) => setPinnedChange(v || null)}
                active={!!pinnedChange}
              />
            </div>
          )}
        </div>

        {cards.length === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <InboxIcon className="h-6 w-6 text-[var(--text-ghost)]" />
            <p className="text-sm text-[var(--text-muted)]">Nothing needs your verdict.</p>
          </div>
        ) : (
          <ul className="flex flex-col gap-3" data-inbox-list>
            {cards.map((card, i) => (
              <Card
                key={card.id}
                card={card}
                focused={i === focus}
                rationale={card.changeId ? rationaleOf.get(card.changeId) : undefined}
                editor={editor}
                rewording={rewording === card.id}
                onFocus={() => setFocus(i)}
                onOpen={() => openCard(card)}
                onAction={(a, rect) => runAction(card, a, rect)}
                onReword={(text) => {
                  if (card.respId && editor) editor.rewordResponsibility(card.respId, text);
                  setRewording(null);
                }}
                onCancelReword={() => setRewording(null)}
                ref={(el) => {
                  if (el) rowRefs.current.set(card.id, el);
                  else rowRefs.current.delete(card.id);
                }}
              />
            ))}
          </ul>
        )}
        {cards.length > 0 && (
          <p className="mt-5 text-xs text-[var(--text-ghost)]">j/k move · a adopt · r reject · e reword · Enter opens</p>
        )}
      </SpecialBody>
      {confirmDrop && (
        <ConfirmPopover
          anchorRect={confirmDrop.rect}
          label="Drop this claim?"
          confirmLabel="Drop"
          onConfirm={() => {
            confirmDrop.run();
            setConfirmDrop(null);
          }}
          onCancel={() => setConfirmDrop(null)}
        />
      )}
    </div>
  );
}

// --- one card ----------------------------------------------------------------------

function Card({
  card,
  focused,
  rationale,
  editor,
  rewording,
  onFocus,
  onOpen,
  onAction,
  onReword,
  onCancelReword,
  ref,
}: {
  card: InboxCard;
  focused: boolean;
  rationale?: string;
  editor: Editor | undefined;
  rewording: boolean;
  onFocus: () => void;
  onOpen: () => void;
  onAction: (a: InboxAction, rect?: DOMRect) => void;
  onReword: (text: string) => void;
  onCancelReword: () => void;
  ref: (el: HTMLLIElement | null) => void;
}) {
  const meta = TIER_META[card.tier];
  const actions = card.actions.filter((a) => editor || !NEEDS_EDITOR.has(a.kind));
  return (
    <li
      ref={ref}
      data-inbox-card={card.id}
      onMouseDown={onFocus}
      className={`rounded-lg border px-4 py-3 transition-colors ${
        focused
          ? "border-[var(--accent)] bg-[var(--surface-inset)]"
          : "border-[var(--border-subtle)] bg-[var(--surface)] hover:border-[var(--border)]"
      }`}
    >
      {/* rail: what kind of item, and where the claim lives. The change it
          belongs to is the filter's job, not the card's. */}
      <div className="flex min-w-0 items-center gap-2.5">
        <span className={`${PILL_BASE} ${meta.cls}`} title={card.title}>
          {pillLabel(card)}
          {card.concern && card.tier === "concern" && <span className="font-normal opacity-80">· {card.concern}</span>}
        </span>
        <span className="flex min-w-0 items-baseline gap-1 truncate text-xs text-[var(--text-muted)]" title={rationale && `Change: ${rationale}`}>
          in
          <button type="button" onClick={onOpen} className={`truncate ${LINK}`}>
            {card.componentName || "Untitled"}
          </button>
          {card.symbolName && (
            <>
              <span className="text-[var(--text-ghost)]">›</span>
              <span className="truncate">{card.symbolName}</span>
            </>
          )}
        </span>
      </div>

      {/* body: the claim in EARS markup — as a word diff when it changed */}
      {TITLED_KINDS.has(card.kind) && (
        <div className="mt-2 text-xs text-[var(--text-tertiary)]">{card.title}</div>
      )}
      <div className="mt-2 font-mono text-sm leading-relaxed text-[var(--text)]">
        {card.kind === "addition" ? (
          <StatementText text={card.after ?? card.statement ?? ""} anchor={ANCHOR_CALM} />
        ) : card.before !== undefined && card.after !== undefined ? (
          <WordDiffText from={stripMarkup(card.before)} to={stripMarkup(card.after)} />
        ) : card.statement ? (
          <StatementText text={card.statement} anchor={ANCHOR_CALM} />
        ) : (
          <span className="italic text-[var(--text-ghost)]">Untitled responsibility</span>
        )}
      </div>

      {/* evidence strip */}
      <EvidenceStrip card={card} onOpen={onOpen} />

      {/* actions */}
      {rewording && card.respId ? (
        <div className="mt-3">
          <RewordEditor
            initial={stripMarkup(card.after ?? card.statement ?? "")}
            onSave={(t) => onReword(serializeEars(t))}
            onCancel={onCancelReword}
          />
        </div>
      ) : (
        actions.length > 0 && (
          <div className="mt-3 flex flex-wrap items-center gap-2">
            {actions.map((a) => (
              <button
                key={a.kind}
                type="button"
                title={a.title}
                className={actionClass(a.kind)}
                onClick={(e) => onAction(a, e.currentTarget.getBoundingClientRect())}
              >
                {a.label}
              </button>
            ))}
          </div>
        )
      )}
    </li>
  );
}

function EvidenceStrip({ card, onOpen }: { card: InboxCard; onOpen: () => void }) {
  const ev = card.evidence;
  const items: React.ReactNode[] = [];

  // Anchor peek: the span, as a link that opens the node page (which renders
  // the highlighted source itself).
  for (const [i, loc] of ev.anchors.slice(0, 3).entries()) {
    items.push(
      <button
        key={`a${i}`}
        type="button"
        onClick={onOpen}
        title={`Open ${loc.pattern}${loc.symbol ? ` · ${loc.symbol}` : ""} on the node page`}
        className={`truncate font-mono ${LINK}`}
      >
        {loc.pattern.split("/").pop()}
        {loc.symbol ? `:${loc.symbol}` : loc.line ? `:${loc.line}` : ""}
      </button>,
    );
  }
  if (ev.anchors.length > 3)
    items.push(<span key="amore" className="text-[var(--text-ghost)]">+{ev.anchors.length - 3} more</span>);

  // Test lane: the flask + count, and the one glyph worth a look.
  if (ev.tests.length > 0 || ev.verdict) {
    const lane = ev.testLane;
    const laneCls =
      lane === "failing" || lane === "hollow"
        ? "text-red-600 dark:text-red-400"
        : lane === "stale"
          ? "text-orange-600 dark:text-orange-400"
          : lane === "probed"
            ? "text-emerald-600 dark:text-emerald-400"
            : "text-[var(--text-secondary)]";
    items.push(
      <span key="tests" className={`inline-flex items-center gap-1 font-mono ${laneCls}`} title={laneTitle(card)}>
        <FlaskConical className="h-3 w-3" />
        {ev.tests.length}
        {lane === "stale" && <RefreshCw className="h-3 w-3" />}
        {lane === "failing" && <X className="h-3 w-3" />}
        {lane === "hollow" && <Crosshair className="h-3 w-3" />}
        {lane === "probed" && <CheckCheck className="h-3 w-3" />}
      </span>,
    );
  }
  if (ev.probe && ev.probeMark !== "none") {
    items.push(
      <span key="probe" className="text-[var(--text-muted)]">
        probed {ev.probe.probes}, {ev.probe.survived} uncaught
      </span>,
    );
  }
  if (ev.dependents && ev.dependents.length > 0) {
    items.push(
      <span key="deps" className="truncate text-[var(--text-muted)]" title={ev.dependents.join(", ")}>
        {ev.dependents.length} dependent{ev.dependents.length === 1 ? "" : "s"}: {ev.dependents.join(", ")}
      </span>,
    );
  }
  if (ev.refusal?.run && ev.refusal.run.length > 0) {
    items.push(
      <span key="run" className="truncate font-mono text-[var(--text-muted)]" title={ev.refusal.run.join("\n")}>
        run: {ev.refusal.run.join(", ")}
      </span>,
    );
  }
  if (ev.closeGate) {
    items.push(
      <span key="gate" className="truncate font-mono text-[var(--text-muted)]">
        {ev.closeGate.file}
        {ev.closeGate.symbol ? `:${ev.closeGate.symbol}` : ""}
        {ev.closeGate.state ? ` · ${ev.closeGate.state}` : ""}
        <span className="ml-1.5 font-sans text-[var(--text-ghost)]">session {ev.closeGate.session.slice(0, 8)}</span>
      </span>,
    );
  }

  return (
    <>
      {items.length > 0 && (
        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">{items}</div>
      )}
      {ev.survivors && ev.survivors.length > 0 && (
        <ul className="mt-1 flex flex-col gap-px text-xs text-[var(--text-muted)]">
          {ev.survivors.map((s, i) => (
            <li key={i} className="truncate" title={s}>
              • {s}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function laneTitle(card: InboxCard): string {
  const v = card.evidence.verdict;
  const n = card.evidence.tests.length;
  const head = `${n} attached test${n === 1 ? "" : "s"}`;
  if (!v) return `${head} — no run recorded`;
  const when = new Date(v.recordedAt * 1000).toLocaleString();
  return `${head} — last run ${v.outcome}${v.stale ? " (stale: code moved since)" : ""}, ${v.cases} case${v.cases === 1 ? "" : "s"}, ${when}`;
}
