/**
 * The session log's fetcher: lists the project's agent sessions
 * (`list_sessions`), follows the most recent one unless the user picked
 * another, and reads it (`read_session`). The hook server's `session-changed`
 * event (payload: the session id) re-lists on any append and re-reads when it
 * is the session on screen. Live = the newest session was written to in the
 * last ten minutes.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { sessionBadge, type SessionSummary, type SessionView } from "./session";

/** A hook session counts as live this long after its last append. */
export const LIVE_WINDOW_SECS = 10 * 60;

export interface SessionLog {
  /** Most recent first. */
  sessions: SessionSummary[];
  /** The session on screen — the picked one, else the most recent. */
  selectedId: string | null;
  /** Pick a session; null follows the most recent again. */
  select: (id: string | null) => void;
  view: SessionView | null;
  /** Open asks + untraced files of the session on screen. */
  badge: number;
  live: boolean;
}

export function useSessionLog({ modelRef }: { modelRef: string | null }): SessionLog {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [picked, setPicked] = useState<string | null>(null);
  const [view, setView] = useState<SessionView | null>(null);
  // Bumped by a session-changed event for the session on screen.
  const [rev, setRev] = useState(0);
  const [lastTouch, setLastTouch] = useState<number | null>(null);

  const list = useCallback(() => {
    if (!modelRef) return;
    invoke<SessionSummary[]>("list_sessions", { refStr: modelRef })
      .then((s) => setSessions(Array.isArray(s) ? s : []))
      .catch(() => {});
  }, [modelRef]);
  useEffect(() => {
    setSessions([]);
    setPicked(null);
    setLastTouch(null);
    list();
  }, [list]);

  // A pick that left the list (project switch, pruned log) falls back to latest.
  const selectedId =
    picked && sessions.some((s) => s.session === picked) ? picked : (sessions[0]?.session ?? null);

  useEffect(() => {
    if (!modelRef || !selectedId) {
      setView(null);
      return;
    }
    let live = true;
    invoke<SessionView>("read_session", { refStr: modelRef, session: selectedId })
      .then((v) => live && setView(v ?? null))
      .catch(() => live && setView(null));
    return () => {
      live = false;
    };
  }, [modelRef, selectedId, rev]);

  useEffect(() => {
    if (!modelRef) return;
    const un = listen<string>("session-changed", (e) => {
      setLastTouch(Math.floor(Date.now() / 1000));
      list();
      if (e.payload === selectedId) setRev((n) => n + 1);
    });
    return () => {
      void un.then((f) => f());
    };
  }, [modelRef, list, selectedId]);

  // Re-evaluate the live window as time passes.
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setTick((n) => n + 1), 30_000);
    return () => clearInterval(t);
  }, []);
  const live = useMemo(() => {
    const last = Math.max(lastTouch ?? 0, sessions[0]?.updatedAt ?? 0);
    return last > 0 && Math.floor(Date.now() / 1000) - last < LIVE_WINDOW_SECS;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lastTouch, sessions, tick]);

  // Never show one session's log under another's id while the read is in flight.
  const shown = view && view.session === selectedId ? view : null;
  const badge = useMemo(() => sessionBadge(shown), [shown]);
  return { sessions, selectedId, select: setPicked, view: shown, badge, live };
}
