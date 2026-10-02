/**
 * Wiki special pages — the cross-cutting surfaces that aren't model content:
 *
 *  - Changes: the whole plan diff — every way `planned` diverges from the
 *    committed model — on one page, grouped per element with before → after
 *    field diffs. The global form of the tree's Changes lens; ordered by most
 *    recent session edit (timestamps borrowed from the session journal), then
 *    by tree position for anything pending from a prior session. Read-only.
 *  - Dark code: the inverse of coverage from the code's side — every file under
 *    a node's boundary that no claim reads into, grouped by the owning node.
 *    Where you eyeball how much is boilerplate versus something load-bearing the
 *    lens is missing.
 *  - Unmapped claims: the same gap from the model's side — committed leaf claims
 *    that say code exists but anchor to nothing. The list behind the coverage
 *    percentage; its complement.
 *  - Session: one agent session's log — each prompt verbatim with the asks the
 *    agent broke it into and their outcome, the files it edited that no ask
 *    accounts for, the claims its edits affected, and the plan elements it
 *    wrote. Glanced at while the agent works, to catch "I didn't ask for that".
 *
 * All are pages, not panels — reached from the status bar counters, left via
 * any link, exactly like Wikipedia's Special:RecentChanges and cleanup
 * categories.
 */

export { RevisionList } from "../widgets/revision-list/RevisionList";
export { ChangesPage } from "./changes/ChangesPage";
export { DarkCodePage } from "./coverage-gaps/DarkCodePage";
export { findUnmappedClaims, UnmappedClaimsPage } from "./coverage-gaps/UnmappedClaimsPage";
export { SessionPage } from "./session/SessionPage";
