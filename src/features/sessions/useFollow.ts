/**
 * Following an agent session in the app: while it runs and isn't paused, the view goes where the session works,
 * once the spot has settled. Going elsewhere yourself or accepting an agent's show pauses it; Resume goes back to
 * where the session is now; the session ending stops it. Also offers, once per session, to follow a session that
 * starts working here. The rules are in `follow.ts`.
 */

import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { dueMove, followReducer, followTarget, offerFor, SETTLE_MS, type SessionBrief } from "./follow";
import { RECENT_SECS, useSessions, type LoggedSession } from "./useSessions";

export function useFollow({
  projectPath,
  selectedId,
  hasNode,
  onMove,
  onEnded,
}: {
  projectPath: string | null;
  /** The node the view has selected: a change the follow didn't make is the developer going elsewhere. */
  selectedId: string | null;
  /** Whether the tree has the node, so a follow never opens a page that doesn't exist yet. */
  hasNode: (id: string) => boolean;
  /** Take the view to the node, keeping the view (map or wiki) the developer is in. */
  onMove: (node: string, session: LoggedSession) => void;
  /** The followed session stopped being live. */
  onEnded: (s: LoggedSession) => void;
}) {
  const [state, dispatch] = useReducer(followReducer, null);
  const [since, setSince] = useState(0);
  const follow = useCallback((session: string) => {
    setSince(Math.floor(Date.now() / 1000) - RECENT_SECS);
    dispatch({ type: "follow", session });
  }, []);
  useEffect(() => dispatch({ type: "stop" }), [projectPath]);

  const { data } = useSessions(state ? projectPath : null, since);
  const session = (state && data?.sessions.find((s) => s.session === state.session)) || null;
  const target = session ? followTarget(session) : null;

  // The target and when it last changed: the view goes there once it has held.
  const targetSince = useRef({ target: null as string | null, at: 0 });
  if (targetSince.current.target !== target) targetSince.current = { target, at: Date.now() };
  const wentTo = useRef<string | null>(null);
  const sessionRef = useRef(session);
  sessionRef.current = session;
  const go = useCallback(
    (node: string) => {
      const s = sessionRef.current;
      if (!s || !hasNode(node)) return;
      wentTo.current = node;
      onMove(node, s);
    },
    [hasNode, onMove],
  );
  useEffect(() => {
    if (!state || state.paused || !target) return;
    const check = () => {
      const node = dueMove({ target, since: targetSince.current.at, wentTo: wentTo.current, now: Date.now() });
      if (node) go(node);
    };
    const wait = Math.max(0, SETTLE_MS - (Date.now() - targetSince.current.at));
    const t = setTimeout(check, wait);
    return () => clearTimeout(t);
  }, [state, target, go]);

  // A selection the follow didn't make is the developer going elsewhere.
  const lastSelected = useRef(selectedId);
  useEffect(() => {
    const moved = lastSelected.current !== selectedId;
    lastSelected.current = selectedId;
    if (moved && state && !state.paused && selectedId !== wentTo.current) dispatch({ type: "away" });
  }, [selectedId, state]);

  const pause = useCallback(() => dispatch({ type: "show" }), []);
  const resume = useCallback(() => {
    dispatch({ type: "resume" });
    // Straight back to where the session is, without waiting for it to settle again.
    if (target) go(target);
  }, [target, go]);
  const stop = useCallback(() => {
    wentTo.current = null;
    dispatch({ type: "stop" });
  }, []);

  // Only a read that has the session can say it ended.
  const ended = !!data && !!state && (!session || !session.live);
  useEffect(() => {
    if (!ended) return;
    dispatch({ type: "ended" });
    if (session) onEnded(session);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per end, with the session as last read
  }, [ended]);

  // Offers: a session that starts working here, once each.
  const [offer, setOffer] = useState<SessionBrief | null>(null);
  const offered = useRef(new Set<string>());
  const stateRef = useRef(state);
  stateRef.current = state;
  useEffect(() => {
    offered.current = new Set();
    setOffer(null);
    if (!projectPath) return;
    const un = listen<string>("session-changed", ({ payload: id }) => {
      if (offered.current.has(id) || stateRef.current?.session === id) return;
      void invoke<SessionBrief>("get_session_brief", { cwd: projectPath, session: id })
        .then((b) => {
          if (!offerFor(b, { offered: offered.current, follow: stateRef.current })) return;
          offered.current.add(b.session);
          setOffer(b);
        })
        .catch(() => {});
    });
    return () => void un.then((f) => f());
  }, [projectPath]);
  const acceptOffer = useCallback(() => {
    if (!offer) return;
    setOffer(null);
    follow(offer.session);
  }, [offer, follow]);
  const dismissOffer = useCallback(() => setOffer(null), []);

  return { state, session, target, sessions: data, follow, pause, resume, stop, offer, acceptOffer, dismissOffer };
}
