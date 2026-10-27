/**
 * Wiki special pages — the cross-cutting surfaces that aren't one node:
 *
 *  - Sessions (the landing page): each agent session, live ones first —
 *    what it is doing as it works, and what its edits proved, planned, or
 *    made stale.
 *  - Dark code: product files no test reaches by import, grouped by the
 *    node that holds them.
 *
 * Both are pages, not panels — reached from the status bar, left via any
 * link, like Wikipedia's Special:RecentChanges.
 */

export { DarkCodePage } from "./coverage-gaps/DarkCodePage";
export { LandingPage } from "./landing/LandingPage";
