/**
 * The inbox's source gatherer: joins the model layers and the plan diff (props
 * the app already holds), the verdict/probe feed (`useTestStatuses`), the
 * fold-refusal ledger (`read_fold_refusals`, re-read on every `model-changed`
 * — the `.scryer/` watcher fires for the ledger file too), the hook server's
 * close-gate events, and the live-session signal (a `hook-touch` in the last
 * ten minutes). Hands the merged inputs to the pure `buildInboxCards` and
 * keeps the per-project "seen" and "dismissed" sets in localStorage.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ClaimProbeStatus, ClaimTestStatus } from "../health";
import {
  buildInboxCards,
  closeGateItems,
  inboxUnread,
  type CloseGateItem,
  type InboxCard,
  type Refusal,
} from "../inbox";
import type { ModelDiff } from "../planDiff";
import type { ScryModel } from "../viewmodel";

/** A hook session counts as live this long after its last touch. */
export const LIVE_WINDOW_SECS = 10 * 60;

/** Most seen-card ids kept per project. */
const SEEN_CAP = 500;

function loadSet(key: string): Set<string> {
  try {
    const raw = localStorage.getItem(key);
    const arr = raw ? (JSON.parse(raw) as unknown) : [];
    return new Set(Array.isArray(arr) ? arr.filter((x): x is string => typeof x === "string") : []);
  } catch {
    return new Set();
  }
}

function saveSet(key: string, set: ReadonlySet<string>) {
  try {
    localStorage.setItem(key, JSON.stringify([...set]));
  } catch {
    /* storage full or unavailable — the sets are a convenience */
  }
}

/** A close-gate item's watermark at arrival: the claim's statement and the
 *  verdict it carried. When either moves, the developer (or the agent) acted
 *  on the claim and the item is spent. */
interface GateEntry {
  item: CloseGateItem;
  statement?: string;
  verdictAt?: number;
}

export interface Inbox {
  /** Every card, dismissed ones excluded, in stream order (unfiltered by pin). */
  cards: InboxCard[];
  unread: number;
  /** A hook session touched code in the last ten minutes. */
  live: boolean;
  seen: ReadonlySet<string>;
  markSeen: (id: string) => void;
  /** Local resolution — approve / dismiss / still-holds. Persisted per project. */
  dismiss: (id: string) => void;
  pinnedChange: string | null;
  setPinnedChange: (id: string | null) => void;
}

export function useInbox({
  model,
  committed,
  planDiff,
  verdicts,
  probes,
  projectPath,
  modelRef,
}: {
  model: ScryModel;
  committed: ScryModel | null;
  planDiff: ModelDiff;
  verdicts: Record<string, ClaimTestStatus>;
  probes: Record<string, ClaimProbeStatus>;
  projectPath: string | null;
  /** The model ref string the storage hook passes to `read_planned` & co. */
  modelRef: string | null;
}): Inbox {
  const storageKey = (what: string) => `scryer:inbox:${what}:${projectPath ?? ""}`;

  // --- refusals: read on open and on every model-changed edge ---------------------
  const [refusals, setRefusals] = useState<Refusal[]>([]);
  const fetchRefusals = useCallback(() => {
    if (!modelRef) return;
    invoke<Refusal[]>("read_fold_refusals", { refStr: modelRef })
      .then((r) => setRefusals(Array.isArray(r) ? r : []))
      .catch(() => {});
  }, [modelRef]);
  useEffect(() => {
    setRefusals([]);
    fetchRefusals();
  }, [fetchRefusals]);
  useEffect(() => {
    if (!modelRef) return;
    const un = listen<string>("model-changed", (e) => {
      if (typeof e.payload === "string" && e.payload !== modelRef) return;
      fetchRefusals();
    });
    return () => {
      void un.then((f) => f());
    };
  }, [modelRef, fetchRefusals]);

  // --- close gate: per session, latest gate wins ----------------------------------
  const [gates, setGates] = useState<GateEntry[]>([]);
  const modelNow = useRef(model);
  modelNow.current = model;
  const verdictsNow = useRef(verdicts);
  verdictsNow.current = verdicts;
  useEffect(() => {
    if (!projectPath) return;
    const un = listen<{ session: string }>("hook-close-gate", (e) => {
      const payload = e.payload as Parameters<typeof closeGateItems>[0];
      if (!payload || typeof payload.session !== "string") return;
      const at = Math.floor(Date.now() / 1000);
      const items = closeGateItems(payload, at);
      const m = modelNow.current;
      const statementOf = (id: string) => {
        for (const h of [...m.nodes, ...m.groups])
          for (const r of h.responsibilities ?? []) if (r.id === id) return r.statement;
        return undefined;
      };
      const entries: GateEntry[] = items.map((item) => ({
        item,
        statement: statementOf(item.id),
        verdictAt: verdictsNow.current[item.id]?.recordedAt,
      }));
      setGates((prev) => [...prev.filter((g) => g.item.session !== payload.session), ...entries]);
    });
    return () => {
      void un.then((f) => f());
    };
  }, [projectPath]);
  useEffect(() => setGates([]), [projectPath]);
  // Drop a gate item once its claim's statement or verdict moved on.
  useEffect(() => {
    setGates((prev) => {
      const statementOf = (id: string) => {
        for (const h of [...model.nodes, ...model.groups])
          for (const r of h.responsibilities ?? []) if (r.id === id) return r.statement;
        return undefined;
      };
      const next = prev.filter((g) => {
        if (g.statement !== undefined && statementOf(g.item.id) !== g.statement) return false;
        const v = verdicts[g.item.id]?.recordedAt;
        if (g.verdictAt !== v) return false;
        return true;
      });
      return next.length === prev.length ? prev : next;
    });
  }, [model, verdicts]);

  // --- live session ---------------------------------------------------------------
  const [lastTouch, setLastTouch] = useState<number | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!projectPath) return;
    const un = listen("hook-touch", () => setLastTouch(Math.floor(Date.now() / 1000)));
    return () => {
      void un.then((f) => f());
    };
  }, [projectPath]);
  useEffect(() => {
    if (lastTouch == null) return;
    const t = setInterval(() => setTick((n) => n + 1), 30_000);
    return () => clearInterval(t);
  }, [lastTouch]);
  const live = useMemo(
    () => lastTouch != null && Math.floor(Date.now() / 1000) - lastTouch < LIVE_WINDOW_SECS,
    // `tick` re-evaluates the window as time passes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [lastTouch, tick],
  );

  // --- seen / dismissed -----------------------------------------------------------
  const [seen, setSeen] = useState<Set<string>>(() => loadSet(storageKey("seen")));
  const [dismissed, setDismissed] = useState<Set<string>>(() => loadSet(storageKey("dismissed")));
  useEffect(() => {
    setSeen(loadSet(storageKey("seen")));
    setDismissed(loadSet(storageKey("dismissed")));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectPath]);

  const closeGate = useMemo(() => gates.map((g) => g.item), [gates]);
  const allCards = useMemo(
    () => buildInboxCards({ model, committed, planDiff, verdicts, probes, refusals, closeGate }),
    [model, committed, planDiff, verdicts, probes, refusals, closeGate],
  );
  const cards = useMemo(() => allCards.filter((c) => !dismissed.has(c.id)), [allCards, dismissed]);

  const markSeen = useCallback(
    (id: string) => {
      setSeen((prev) => {
        if (prev.has(id)) return prev;
        const next = new Set(prev);
        next.add(id);
        // Bounded, oldest-first eviction: ids of cards that left the stream
        // are dead weight, but pruning against the CURRENT cards would forget
        // sources that load asynchronously (refusals, verdicts) on every open.
        for (const old of next) {
          if (next.size <= SEEN_CAP) break;
          next.delete(old);
        }
        saveSet(storageKey("seen"), next);
        return next;
      });
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [projectPath],
  );
  const dismiss = useCallback(
    (id: string) => {
      setDismissed((prev) => {
        const next = new Set(prev);
        next.add(id);
        saveSet(storageKey("dismissed"), next);
        return next;
      });
      // A dismissed close-gate item leaves the session's list too.
      setGates((prev) => prev.filter((g) => `close-gate:${g.item.session}:${g.item.file}:${g.item.id}` !== id));
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [projectPath],
  );

  const [pinnedChange, setPinnedChangeRaw] = useState<string | null>(null);
  // A pin to a change that closed silently releases.
  const pinned = pinnedChange && (model.changes ?? []).some((c) => c.id === pinnedChange) ? pinnedChange : null;
  const setPinnedChange = useCallback((id: string | null) => setPinnedChangeRaw(id), []);

  const unread = useMemo(() => inboxUnread(cards, seen), [cards, seen]);

  return { cards, unread, live, seen, markSeen, dismiss, pinnedChange: pinned, setPinnedChange };
}
