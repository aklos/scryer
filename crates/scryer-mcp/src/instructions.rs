/// Connect-time instructions — the ROOT rule. Always loaded, so it carries
/// only the loop and the slugs of the rules that govern each step; the bodies
/// live in `scryer_core::rules` and are fetched with `get_rules {id}` when the
/// agent reaches that step. Every tool description ends with its own `Rules:`
/// line the same way.
pub(crate) const INSTRUCTIONS: &str = "\
This project has a scryer architecture model alongside its code: a tree of what each part is \
RESPONSIBLE for, mapped to the source that implements it and to the TESTS attached to each claim. \
It is the user's authored spec, not optional background. While a model exists you work through it: \
plan a change in the model FIRST, then write code and tests to match.\n\
\n\
RULES ARE FETCHED, NOT ASSUMED. Every tool description ends with a `Rules:` line naming the slugs \
that govern it, and a rule cites others by slug in double square brackets. Before you use a tool in a way one of those \
rules governs, or make a modeling judgment, fetch the rule: `get_rules {id: \"slug-a,slug-b\"}` \
(`get_rules {}` lists them all). Never infer the conventions from existing nodes.\n\
\n\
## Every prompt\n\
The hook logs each user prompt and names it (`p3`). Break it into asks with `file_asks` before \
anything else, do what the asks cover and nothing beyond it, and end each one delivered \
(`resolve_ask {id, claims}`: verified claims on code you edited), answered, or descoped with a \
reason. The Stop hook names whatever is still open. Never ask the user to review or approve: \
finish the work. No silent passes or stubs: an unfinished ask is descoped saying what is left. \
[[ask-ledger]]\n\
\n\
## Every task beyond a one-line fix\n\
1. ORIENT — `orient {task, files}` for a coding task; `get_health` then `read_model` for a \
model-building one. Honor every directive it returns. [[loop-orient]]\n\
2. PLAN — author the change into the model before writing code; your plan writes land in this \
session's change automatically [[change-ledger]]. Only changes that alter what the model claims \
need plan entries. A container declares a `style` \
only when its code actually has that shape; never guess one. [[loop-plan]] [[proportionality]] \
[[styles]]\n\
3. BUILD — implement claim by claim, each testable (When/While/If) claim with its test in the \
project's own suite. Placement is given, not chosen: `scaffold {node_id}` and `orient {files}` \
name each planned component's directory, layer and allowed imports. [[loop-build]] [[styles]]\n\
4. CLOSE — `mark_implemented` with `anchors` and `tests` in the same call; the fold is gated on a \
passing verdict, so run the tests with a JUnit reporter and `ingest_test_report` first. Then \
`get_test_radius`, `flag_drift` and `resolve_drift` (drift verdicts are yours, never the \
user's), `reconcile_drift`, then link the claims to their ask. Leave no planned entry \
unfolded. [[loop-close]] [[drift-directions]]\n\
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
- `N structural violations` in a status line counts real imports and files that break a \
declared style, import cycles between components, and containers that declare no style at all. \
Fixing them means moving code, never un-declaring the style, dropping a layer, or deleting the \
link; an unstyled container is resolved only by the user declaring the style its code has. \
[[styles]]\n\
\n\
Every tool takes an optional `project` (absolute path) that defaults to the working directory. \
Schema version is `0.3`.\n\
";
