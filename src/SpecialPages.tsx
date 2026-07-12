/**
 * Wiki special pages — the cross-cutting surfaces that aren't model content:
 *
 *  - Changes: the whole plan diff — every way `planned` diverges from the
 *    committed model — on one page, grouped per element with before → after
 *    field diffs. The global form of the tree's Changes lens; ordered by most
 *    recent session edit (timestamps borrowed from the session journal), then
 *    by tree position for anything pending from a prior session.
 *  - Needs review: the maintenance-category index. Every observation awaiting
 *    a human verdict, grouped by kind, with the verdict actions inline. An
 *    empty page means the model is trustworthy.
 *  - Dark code: the inverse of coverage from the code's side — every file under
 *    a node's boundary that no claim reads into, grouped by the owning node.
 *    Where you eyeball how much is boilerplate versus something load-bearing the
 *    lens is missing.
 *  - Unmapped claims: the same gap from the model's side — committed leaf claims
 *    that say code exists but anchor to nothing. The list behind the coverage
 *    percentage; its complement.
 *
 * All are pages, not panels — reached from the status bar counters, left via
 * any link, exactly like Wikipedia's Special:RecentChanges and cleanup
 * categories.
 */

export { RevisionList } from "./special/RevisionList";
export { ChangesPage } from "./special/ChangesPage";
export type { ReviewIndex } from "./special/NeedsReviewPage";
export { buildReviewIndex, NeedsReviewPage } from "./special/NeedsReviewPage";
export { DarkCodePage } from "./special/DarkCodePage";
export { findUnmappedClaims, UnmappedClaimsPage } from "./special/UnmappedClaimsPage";
