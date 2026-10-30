/**
 * Navigation — what the wiki is showing: one node, or one of the special
 * pages that aren't a single element.
 */

export type SpecialPage = "dark" | "session";

export type Selected =
  | { kind: "node"; id: string }
  // Wiki special pages — Sessions (`session`) and Dark code
  // (App routes these).
  | { kind: "special"; id: SpecialPage };
