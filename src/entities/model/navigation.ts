/**
 * Navigation — what the wiki is showing: one node, one group, or one of the
 * special pages that aren't a single element.
 */

export type SpecialPage = "dark" | "session";

export type Selected =
  | { kind: "node"; id: string }
  | { kind: "group"; id: string }
  // Wiki special pages — Since you last looked (`session`) and Dark code
  // (App routes these).
  | { kind: "special"; id: SpecialPage };
