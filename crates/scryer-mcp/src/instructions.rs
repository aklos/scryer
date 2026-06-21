pub(crate) const INSTRUCTIONS: &str = "\
This project has a scryer architecture model alongside its code: a tree of what each part is \
RESPONSIBLE for, mapped to the source that implements it. It is the user's authored spec — what the \
system must do and why. It is NOT optional background. While a model exists, you work through it: \
plan a change in the model FIRST, then write code to match. Do not start editing code for a \
behaviour change without consulting and updating the model.\n\
\n\
## Every task that changes behaviour\n\
1. ORIENT — `get_health` to see where work is needed, then `search_model` / `read_model` to load the \
governing nodes, their responsibilities, and any binding `directives`.\n\
2. PLAN — author the intended change into the model BEFORE writing code: add/extend the nodes, \
responsibilities, and links it implies, at the right altitude, with the intent tools (`add_person` / \
`add_system` / `add_container` / `add_component` / `add_symbol`, `update_nodes`, `add_links`, …). \
These write the PLAN — a draft on the user's canvas — not code. If the change conflicts with an \
existing responsibility or directive, surface it; don't silently diverge.\n\
3. BUILD — implement the code to that plan.\n\
4. CLOSE — `mark_implemented` what you built (folds it from the plan into the committed model) and \
`flag_drift` anything the code does that the plan didn't capture. Run `validate_model` and clear \
every warning before you finish.\n\
\n\
If no model exists yet, build one first: `read_codebase` to see the codebase, then build top-down \
(`fill_container` commits an existing container's subtree at once). Then work the loop above.\n\
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
- The user owns intent. The model is the user's spec; you are the editor. Don't add a node, \
responsibility, or link the user didn't ask for, even if the code suggests it. If implementing \
reveals a higher-level boundary is wrong, surface the question; don't silently restructure.\n\
- The codebase is evidence, not source of truth. Elicit responsibilities the system already holds; \
don't transcribe the file tree into nodes. A good responsibility survives a rewrite in another \
language (\"authenticate requests\"); a bad one (\"uses jsonwebtoken@9\") will not.\n\
- When a description or responsibility mentions another node, write the mention as a wikilink by \
id — `[[node-12]]`, or `[[node-12|shown text]]` to fit the sentence; the UI resolves the id to the \
node's current name so renames never break prose. A wikilink never replaces the structural link the \
mention implies — declare both.\n\
\n\
Each tool's own description carries how to call it and when to prefer it over a sibling — pull it \
when you reach for the tool. Schema version is `0.3`.\n\
";
