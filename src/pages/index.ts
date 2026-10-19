/**
 * Wiki special pages — the cross-cutting surfaces that aren't one node:
 *
 *  - Since you last looked (the landing page): per agent session since the
 *    project was last opened, what its edits proved, planned, or made stale.
 *  - Dark code: product files no test reaches by import, grouped by the
 *    node that holds them.
 *
 * Both are pages, not panels — reached from the status bar, left via any
 * link, like Wikipedia's Special:RecentChanges.
 */

export { DarkCodePage } from "./coverage-gaps/DarkCodePage";
export { LandingPage } from "./landing/LandingPage";
