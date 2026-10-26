/**
 * Comment threads: notes people and agents leave each other on a node,
 * closed explicitly the way issues are. Mirrors Rust `domain::comments`.
 */

export type ThreadStatus = "open" | "addressed";

export interface CommentEvent {
  /** Unix milliseconds. */
  at: number;
  author: "human" | "agent";
  text: string;
  /** The node the thread is on — the opening event only. */
  node?: string;
  /** Where that node was, in words. */
  anchor?: string;
  /** A status change this event makes; absent on a plain reply. */
  status?: ThreadStatus;
}

export interface Thread {
  id: string;
  node: string;
  anchor: string;
  status: ThreadStatus;
  events: CommentEvent[];
}
