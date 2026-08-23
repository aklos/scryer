pub(crate) const INSTRUCTIONS: &str = "\
This project has a scryer architecture model alongside its code: a tree of what each part is \
RESPONSIBLE for, mapped to the source that implements it, and to the TESTS attached to each claim. \
It is the user's authored spec — what the system must do and why. It is NOT optional background. \
While a model exists, you work through it: plan a change in the model FIRST, get the user's \
sign-off, then write code — and its tests — to match. Do not start editing code for a behaviour \
change without consulting the model, and update the model first when the change alters what it \
claims (the Proportionality section below draws that line).\n\
\n\
A claim either HAS tests attached or it DOESN'T, and that binary is the model's PRIMARY signal: an \
attached test is the one artifact that attempts to hold the code to the claim's words, so \
attaching tests as you implement is not a follow-up chore — it is part of implementing. Scryer \
never RUNS tests itself, but it tracks their VERDICTS: after a run, `ingest_test_report {path}` \
reads the runner's JUnit XML and records each attached test's result against its claim — one call \
per report file — fingerprint-keyed so a later edit to the implementation or the test flips the \
verdict to stale. `get_test_radius` answers \"what needs running\": exactly the test files whose \
claims hold missing or stale verdicts, never the whole suite. The `untested` count in every status \
line is your standing work signal; `tests: N failing/stale` joins it only when something needs \
attention.\n\
\n\
## Every task that changes behaviour\n\
1. ORIENT — figure out which phase you're in first. `get_health` reports how well the COMMITTED \
model maps to code, so it is the right entry point only once code exists. If committed is empty (a \
design-first model whose whole architecture lives in the plan, before anything is built), it has \
nothing to report — `get_pending` and `read_model` show the authored plan; never read an empty \
health report as \"nothing authored\". For a CODING task, start from `orient {task, files}` — one \
call returns the governing nodes, their claims and binding directives, the scoped pending items and \
drift, the matching modeling rules, and which phase you're in. `locate {file, symbol?}` is its \
single-file sibling. The whole-model reads are for model-building sessions: lead with `get_health` \
to see where work is needed, then `search_model` / `read_model` to load the \
governing nodes, their responsibilities, and any binding `directives`. Directives are user-authored, \
read-only HOW-constraints (\"must\"/\"never\" rules). They attach to a responsibility OR to a node, and \
node-level directives CARRY DOWN: a node is bound by its own plus every ancestor's. `read_model` \
returns the inherited set in `inheritedDirectives`; honor all of them, and never edit a directive \
unless the user explicitly asks you to.\n\
2. PLAN — author the intended change into the model BEFORE writing code: add/extend the nodes, \
responsibilities, and links it implies, at the right altitude, with the intent tools (`add_person` / \
`add_system` / `add_container` / `add_component` / `add_symbol`, `update_nodes`, `add_links`, …). \
These write the PLAN — a draft on the user's canvas — not code. If the change conflicts with an \
existing responsibility or directive, surface it; don't silently diverge. When other work may share \
the plan (parallel sessions, a dev on the canvas), first name YOUR task: `set_change {rationale}` \
opens a named change and your plan writes tag to it automatically, keeping workstreams separable — \
resume a prior session's change with `set_change {change_id}` (listed in `get_pending`'s \
`openChanges`). Quick serial edits can skip this and go unfiled.\n\
3. SIGN-OFF — the plan is a proposal on the user's canvas; before building it, tell the user \
what you planned and get their go-ahead. The user owns the spec — skip this only when they already \
approved the change in this conversation or explicitly told you to run ahead.\n\
4. BUILD — implement the code to that plan, responsibility by responsibility, WITH ITS TESTS: for \
each testable (When/While/If) claim you implement, the statement already names the trigger, state, \
or failure to arrange and the response to assert — write that test in the project's own suite as \
part of the same work. On symbol-level claims the test is MANDATORY (rule 22); at higher altitudes \
attach the kind of test that fits (component → integration, container → API/service, system → \
end-to-end).\n\
5. CLOSE — `mark_implemented` what you built (folds it from the plan into the committed model), \
passing `anchors` so fold + anchor is one atomic call, and `flag_drift` anything the code does that \
the plan didn't capture. ATTACH each claim's test with `tests` in the same call \
(`pattern` = test file, `symbol` = the test function) — the claim then reads as tested in \
health, and the test's fingerprint is watched like any anchor. For a claim in a When/While/If form \
the test is EXPECTED — mandatory on symbol hosts (rule 22): the statement already names the \
trigger/state/failure to arrange and the response to assert, so write that test as part of the \
build and attach it here — health counts such claims without one as `untested`. Ubiquitous claims \
stay a judgment call; never \
attach a test that doesn't genuinely exercise its claim just to clear the counter. The fold response ends with a scoped post-flight (what's still pending on \
the node, unanchored claims, new warnings) — act on it; a separate `validate_model` run is for \
structural sessions that touched many nodes, not for every fold. You need not finish a whole node \
before committing: when you build \
in layers, fold only the responsibilities you actually built (`mark_implemented` accepts \
`responsibilityIds`) and leave the rest in the plan. Committing a structural node asserts only that \
its boundary exists, never that its unbuilt descendants do — so commit the skeleton you built and let \
the pending work roll up. A node whose subtree mixes built and unbuilt work shows an intermediate \
completeness in `get_health` — anchored primitives over authored ones — the honest state for a \
layered build, never a reason to withhold the skeleton (rules 18-19). Never anchor a claim you have \
not implemented: anchoring is the build checkpoint, which is what makes the completeness figure \
trustworthy. If you opened a change, `mark_implemented {change}` folds exactly its entries; when \
the last one folds the change closes and its rationale is recorded in the history log. Then \
VERIFY: `get_test_radius` names the test files your change invalidated (missing or stale \
verdicts) — run exactly those with the runner's JUnit reporter on and `ingest_test_report` each \
report file, so the claims' verdicts are current before you move on. Then reconcile and continue: \
`reconcile_drift` advances the anchor once the scope is clean, and the next responsibility starts \
the loop again at BUILD.\n\
\n\
If no model exists yet, build one first: `read_codebase` to see the codebase, then build top-down \
(`fill_container` commits an existing container's subtree at once). Then work the loop above.\n\
\n\
## Proportionality — what earns a plan entry\n\
Match the ceremony to the change; the full loop above is for changes that alter what the model \
claims. NO plan entry is needed for: bugfixes that restore behaviour the model already claims, pure \
refactors (moved-but-unchanged symbols re-anchor themselves), or docs/tests/chores. A plan entry IS \
needed for: new, changed, or removed responsibilities; new nodes; changed links. For exploratory \
spikes, spiking freely is legitimate — but before the result is kept, reconcile via `flag_drift` so \
the model catches up; drift exists precisely so the code can lead when it must.\n\
\n\
One obligation is NEVER waived, because it is cheap only in the moment: if you changed the behaviour \
of an anchored symbol, confirm or reword its claims BEFORE you finish, while the diff is still in \
your context — `locate {file, symbol}` returns just those claims, a few lines to check. Deferred, \
the same reconciliation costs a later session thousands of tokens to reconstruct.\n\
\n\
## How the model is stored\n\
Two layers on disk: the committed `model` (the source of truth — what the code is believed to \
satisfy) and the `planned` draft (what you and the canvas edit). Their difference is the PLAN — the \
model→code work queue (`get_pending`). Authoring tools write the PLAN; the committed model changes \
only when work is implemented and folds in (`mark_implemented`), or when you extract from code that \
already exists. Reads return the PLAN layer by default, so what you read back reflects what you just \
authored.\n\
\n\
## Drift — keep the model and code in sync\n\
Drift is a code change the PLAN does not account for. That is why you plan first: code you change in \
service of a pending plan item is expected churn and stays silent, but changing already-mapped code \
with no plan item to explain it is flagged the moment you make it — the signal to either revert a \
mistake or put the change in the plan. `get_drift` reports the scopes whose code changed since the \
last reconcile; `read_model` them, compare, `flag_drift`, then `reconcile_drift` to advance the \
anchor.\n\
\n\
## Binding constraints\n\
- The modeling rules are AUTHORITATIVE. Before any modeling judgment — what earns a symbol, how to \
pitch a responsibility's altitude, when a group is right, how links propagate — call \
`get_rules{topic}` and follow it. Never infer the conventions from existing nodes.\n\
- The user owns intent. The model is the user's spec; you are the editor. Translating the change \
the user asked for into the model deltas it implies — the nodes, responsibilities, and links that \
express it — is your job; do it without asking. Inventing scope BEYOND the request is not: don't \
add elements the code merely suggests, and if implementing reveals a higher-level boundary is \
wrong, surface the question rather than silently restructuring.\n\
- The codebase is evidence, not source of truth. Elicit responsibilities the system already holds; \
don't transcribe the file tree into nodes. A good responsibility survives a rewrite in another \
language (\"authenticate requests\"); a bad one (\"uses jsonwebtoken@9\") will not.\n\
- When a description or responsibility names another node, declare the structural link the mention \
implies — the prose mention and the structural link are distinct; declare both.\n\
- Write to be scanned, and name for a newcomer. A node name is plain domain vocabulary — no \
codenames, abbreviations, or cleverness. A responsibility is ONE terse verb-led clause: no mechanism \
words, no trailing \"so that…\" purpose clause. Statements speak EARS with the node as implicit \
subject — condition first (\"When <trigger>, …\" / \"If <condition>, then …\"), response last — \
with **bold** on the keywords and the response verb, and nothing else (rules 15, 17, 21).\n\
- CONCERNS are the model's third axis: each responsibility may carry at most one `concern` slug \
naming the cross-cutting accountability it serves (`auth`, `persistence`, `idempotency`, …), and \
the model registers every slug in use — the lens the user scans by. Tag as you author \
(`{statement, concern}` wherever statements go), reuse registry slugs before minting, and leave \
core domain flow untagged. Rule 20 governs.\n\
\n\
Each tool's own description carries how to call it and when to prefer it over a sibling — pull it \
when you reach for the tool. Schema version is `0.3`.\n\
";
