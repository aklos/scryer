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
 *  - Inbox: the in-session queue — every item awaiting the developer's verdict
 *    (amendments, vagrants, stale, survivors, failing, contract rewords, refused
 *    folds, close-gate items) as one live stream ordered by risk then recency.
 *
 * All are pages, not panels — reached from the status bar counters, left via
 * any link, exactly like Wikipedia's Special:RecentChanges and cleanup
 * categories.
 */

export { RevisionList } from "../widgets/revision-list/RevisionList";
export { ChangesPage } from "./changes/ChangesPage";
export type { ReviewIndex } from "./needs-review/NeedsReviewPage";
export { buildReviewIndex, NeedsReviewPage } from "./needs-review/NeedsReviewPage";
export { DarkCodePage } from "./coverage-gaps/DarkCodePage";
export { findUnmappedClaims, UnmappedClaimsPage } from "./coverage-gaps/UnmappedClaimsPage";
export { InboxPage } from "./inbox/InboxPage";
