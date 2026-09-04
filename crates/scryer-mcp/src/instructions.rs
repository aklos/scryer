/// Connect-time instructions — the ROOT rule. Always loaded, so it carries
/// only the loop and the slugs of the rules that govern each step; the bodies
/// live in `scryer_core::rules` and are fetched with `get_rules {id}` when the
/// agent reaches that step. Every tool description ends with its own `Rules:`
/// line the same way.
pub(crate) const INSTRUCTIONS: &str = "\
This project has a scryer architecture model alongside its code: a tree of what each part is \
RESPONSIBLE for, mapped to the source that implements it and to the TESTS attached to each claim. \
It is the user's authored spec, not optional background. While a model exists you work through it: \
plan a change in the model FIRST, get the user's sign-off, then write code and tests to match.\n\
\n\
RULES ARE FETCHED, NOT ASSUMED. Every tool description ends with a `Rules:` line naming the slugs \
that govern it, and a rule cites others by slug in double square brackets. Before you use a tool in a way one of those \
rules governs, or make a modeling judgment, fetch the rule: `get_rules {id: \"slug-a,slug-b\"}` \
(`get_rules {}` lists them all). Never infer the conventions from existing nodes.\n\
\n\
## Every task beyond a one-line fix\n\
1. OPEN — `open_change {rationale}` first, whatever the task; plan writes are refused while no \
change is open, and the rationale outlives the work in the history log. [[change-ledger]]\n\
2. ORIENT — `orient {task, files}` for a coding task; `get_health` then `read_model` for a \
model-building one. Honor every directive it returns. [[loop-orient]]\n\
3. PLAN — author the change into the model before writing code. Only changes that alter what the \
model claims need plan entries; the change stays open either way. [[loop-plan]] [[proportionality]]\n\
4. SIGN-OFF — tell the user what you planned and get their go-ahead; record it with \
`sign_off`. [[loop-sign-off]]\n\
5. BUILD — implement claim by claim, each testable (When/While/If) claim with its test in the \
project's own suite. [[loop-build]]\n\
6. CLOSE — `mark_implemented` with `anchors` and `tests` in the same call; the fold is gated on a \
passing verdict, so run the tests with a JUnit reporter and `ingest_test_report` first. Then \
`get_test_radius`, `flag_drift`, `reconcile_drift`. A change that filed nothing closes with \
`close_change`. [[loop-close]]\n\
\n\
If no model exists yet, build one first from the code. [[generation-fill]]\n\
\n\
## How the model is stored\n\
Two layers: the committed `model` (what the code is believed to satisfy) and the `planned` draft \
(what you and the canvas edit). Their difference is the plan, the model→code work queue; authoring \
tools write the plan, and reads return it by default. [[model-layers]]\n\
\n\
## Binding constraints\n\
- The user owns intent; you are the editor. [[user-owns-intent]]\n\
- The codebase is evidence, not the source of truth. [[codebase-as-evidence]]\n\
- Directives are the user's binding HOW-constraints; read them, never write them unasked. \
[[directives-binding]]\n\
- Code you change with no plan item to explain it is drift; plan first so it stays silent. \
[[drift-first]]\n\
- Statements speak EARS, one terse verb-led clause, names in plain domain vocabulary, at most one \
concern each. [[statement-ears]] [[scanning]] [[naming]] [[concerns]]\n\
- A claim has a test attached or it doesn't; that binary is the model's primary signal, and the \
`untested` count in every status line is your standing work. [[test-attachment]] [[test-verdicts]]\n\
\n\
Every tool takes an optional `project` (absolute path) that defaults to the working directory. \
Schema version is `0.3`.\n\
";
