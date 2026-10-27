/**
 * Following an agent session: while on, each edit the session makes in a
 * component takes the wiki and the map to it, and the view switches to show
 * the change — the map when it changes the shape around the node, the wiki
 * when it changes the node's claims or description. Going anywhere else
 * yourself hands navigation back, and so does the session ending.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { Selected } from "../../entities/model/navigation";
import type { ScryModel } from "../../entities/model/viewmodel";
import { followView, stepView, type FollowView } from "./followView";
import { followTarget, RECENT_SECS, useSessions, type LoggedSession } from "./useSessions";

export function useFollow({
  projectPath,
  model,
  selected,
  hasNode,
  onGo,
  onView,
  onEnded,
}: {
  projectPath: string | null;
  /** The tree as last read; a change at the followed node picks the view. */
  model: ScryModel;
  selected: Selected | null;
  /** Whether the tree has this node, so a follow never opens a page that
   *  does not exist yet. */
  hasNode: (id: string) => boolean;
  /** Take the wiki and map to a node. */
  onGo: (id: string) => void;
  /** Switch to the view that shows the session's latest change. */
  onView: (view: FollowView) => void;
  /** The followed session stopped being live. */
  onEnded: (s: LoggedSession) => void;
}) {
  const [following, setFollowing] = useState<string | null>(null);
  const [since, setSince] = useState(0);
  const follow = useCallback((session: string | null) => {
    setSince(Math.floor(Date.now() / 1000) - RECENT_SECS);
    setFollowing(session);
  }, []);
  const stop = useCallback(() => setFollowing(null), []);
  useEffect(() => setFollowing(null), [projectPath]);

  const { data } = useSessions(following ? projectPath : null, since);
  const session = (following && data?.sessions.find((s) => s.session === following)) || null;
  const target = session ? followTarget(session) : null;

  // The node the follow last went to; a selection elsewhere is yours.
  const wentTo = useRef<string | null>(null);
  useEffect(() => {
    wentTo.current = null;
  }, [following]);
  useEffect(() => {
    if (!following || !target || target === wentTo.current || !hasNode(target)) return;
    wentTo.current = target;
    onGo(target);
  }, [following, target, hasNode, onGo]);
  // The tree as of the last switch: a node the session adds may land in the
  // tree before its edit names it, so the diff runs from here, not from the
  // previous read.
  const base = useRef(model);
  const lastStep = session?.steps?.[session.steps.length - 1];
  const stepKey = lastStep ? `${lastStep.kind}@${lastStep.at}` : "";
  const seenStep = useRef("");
  useEffect(() => {
    base.current = model;
    seenStep.current = stepKey;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- a new follow starts from what is on screen
  }, [following]);
  useEffect(() => {
    if (!following || !target) return;
    const fresh = stepKey !== seenStep.current;
    seenStep.current = stepKey;
    const view = (fresh ? stepView(lastStep) : null) ?? followView(base.current, model, target);
    if (!view) return;
    base.current = model;
    onView(view);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `lastStep` is keyed by `stepKey`
  }, [following, target, model, stepKey, onView]);

  // Only a change of selection counts: the commit that goes still shows the
  // page you were on.
  const lastSelected = useRef(selected);
  useEffect(() => {
    const moved = lastSelected.current !== selected;
    lastSelected.current = selected;
    if (!moved || !following || !wentTo.current) return;
    if (!(selected?.kind === "node" && selected.id === wentTo.current)) setFollowing(null);
  }, [following, selected]);

  // Only a read that has the session can say it ended.
  const ended = !!data && (!session || !session.live);
  useEffect(() => {
    if (!following || !ended) return;
    setFollowing(null);
    if (session) onEnded(session);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per end, with the session as last read
  }, [following, ended]);

  return { following, follow, stop, session, target, sessions: data };
}
