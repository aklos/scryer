/**
 * Agents' directions to the view: a direction raises a notice naming the session that sent it and where it points,
 * and the developer accepts it to go there. It never moves the view by itself: an agent shows something to talk
 * about it, so going there is the developer's act, and it pauses any session they follow.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** A direction as the app host sends it (mirrors Rust `app_view::Direction`). */
export interface Direction {
  session?: string;
  /** What the notice calls the sender: its task, else "an agent". */
  sender: string;
  node: string;
  view?: "map" | "wiki";
  claims?: string[];
  fields?: string[];
  nodes?: string[];
  note?: string;
  at: number;
}

export interface Notice {
  direction: Direction;
  /** Whether the view went there (a link the developer clicked), or the notice offers to. */
  moved: boolean;
}

/** What a direction raises: a notice offering to go where it points; none for a repeat of the last direction, or
 *  one to a node the tree lacks (not derived yet, or gone). */
export function noticeFor(d: Direction, { hasNode, lastAt }: { hasNode: (id: string) => boolean; lastAt: number }): Notice | null {
  if (d.at === lastAt || !hasNode(d.node)) return null;
  return { direction: d, moved: false };
}

export function useDirections({
  hasNode,
  onDirect,
}: {
  /** Whether the tree has the node: a direction to one it lacks (not derived yet, or gone) is dropped. */
  hasNode: (id: string) => boolean;
  /** Take the view where the direction points. */
  onDirect: (d: Direction) => void;
}) {
  const [notice, setNotice] = useState<Notice | null>(null);
  const onDirectRef = useRef(onDirect);
  onDirectRef.current = onDirect;
  const hasNodeRef = useRef(hasNode);
  hasNodeRef.current = hasNode;

  // One write can reach the watcher as several events; a direction is acted on once.
  const lastAt = useRef(0);
  const receive = useCallback((d: Direction) => {
    const n = noticeFor(d, { hasNode: hasNodeRef.current, lastAt: lastAt.current });
    if (!n) return;
    lastAt.current = d.at;
    setNotice(n);
  }, []);

  useEffect(() => {
    const un = listen<Direction>("view-direction", ({ payload }) => receive(payload));
    return () => void un.then((f) => f());
  }, [receive]);

  /** Go where a link the developer clicked points; the notice still names who sent it. */
  const follow = useCallback((d: Direction) => {
    lastAt.current = d.at;
    setNotice({ direction: d, moved: true });
    onDirectRef.current(d);
  }, []);

  /** Accept a notice: go where it points. */
  const go = useCallback((d: Direction) => {
    setNotice(null);
    onDirectRef.current(d);
  }, []);
  const dismiss = useCallback(() => setNotice(null), []);

  return { notice, go, follow, dismiss, receive };
}
