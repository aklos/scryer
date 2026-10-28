---
name: scryer
description: How to plan, build and verify code in a project Scryer reads. Scryer derives the architecture from the code and its claims from behaviour tests, so a plan is a set of todo tests and "done" is a passing, ingested test report. Use for any coding task here — new behaviour, a fix, a refactor, a move between modules — and whenever orient, locate or a [scryer] note mentions claims, layers or other sessions.
---

<!-- Written by `scryer-mcp setup on`; `setup off` removes it. -->

# Working in a Scryer project

Scryer reads this repository and derives its C4 tree: containers from packaging, components from modules and directories, links from imports. Claims come from behaviour tests: **a test's description is a claim** about the code it tests, and its status (proven, stale, failing, todo) comes from test reports. You never edit a model. There is none. You write code and tests; Scryer reads them.

The loop: **orient → write the claims as todo tests → build → run what the change reaches → ingest the report → probe what you doubt.**

## 1. Orient

Before touching code, call `orient` with the task in 3–5 words and the files you expect to touch. The task names your session on the developer's Sessions page, where they follow it as it works: `queue offline saves`, not a sentence. Per file you get:

- where it sits (`system / container / component`), its **layer** with what that layer may import, and any style violation already charged to the file,
- the module description and the claims that already speak for it, with status,
- `stubs`: the test file where a new claim for this code goes.

`locate {file}` is the one-file form with every claim. `search_model` and `read_model` find and read nodes when you don't know the file yet.

## 2. Plan as todo tests

Your plan is the list of claims you will make true, each written as a todo test in the file `stubs` names, before you build. A todo test is work to do; the user reads the list instead of a plan document.

A claim is one EARS sentence, subject dropped, condition first, one trigger and one response:

- always true: `Rejects a refund larger than the captured amount.`
- `When <trigger>, <response>`: `When a capture settles, posts it to the ledger.`
- `While <state>, <response>`: `While a dispute is open, holds the disputed amount.`
- `If <unwanted condition>, then <response>`: `If the bank times out, then retries twice and marks the payout pending.`

A cross-cutting concern is a trailing tag: `… [persistence]`, `… [auth]`. `[manual]` takes a test out of the claims.

The todo form per language:

```rust
/// When a capture settles, posts it to the ledger.
#[test]
#[ignore]
fn posts_settled_capture() {
    todo!()
}
```

```ts
it.todo("When a capture settles, posts it to the ledger");
```

```python
@pytest.mark.skip
def test_posts_settled_capture():
    """When a capture settles, posts it to the ledger."""
```

```csharp
[Fact(Skip = "todo", DisplayName = "When a capture settles, posts it to the ledger")]
public void PostsSettledCapture() { }
```

**Level.** The test's level is the claim's level. A unit test claims for its component, an integration test (at a deployable's boundary) for its container, an e2e test for the whole system. Write each claim at the level it speaks for; `.scryer/structure.toml` `[levels]` says which paths are which.

## 3. Build

Make the todos pass and turn each into a real test: drop `#[ignore]` / `.todo` / `skip`, keep the sentence. If the behaviour you built differs from the sentence, change the sentence; it must stay true.

**Where code goes.** A container may declare an architecture style in `.scryer/structure.toml` (hexagonal, feature-sliced, core-shell, library, …) with layers. Respect what the file's layer may import. When a file breaks its container's style, **fix it by moving code** to the layer it belongs in. Never change the style, the layers or `structure.toml` to make a finding go away, unless the user asks. A container declared `tooling = true` (demos, harnesses, dev apps) is not held to style rules; orient says so.

**How to organise it.** Scryer draws each module or directory as a component, so how you organise code is what the reader sees:

- **A component is one job behind one entry file.** Name it for what it does (`billing`, `claim_text`, `session_store`), never for its layer (`domain`, `services`, `utils`). A layer is where a component sits, not a component.
- **A component sits in one layer.** In a styled container, end the first paragraph of its module doc with its layer: `//! Which agent to run, and how to spawn it. [domain]`. Don't create layer folders (`domain/`, `infrastructure/`) to hold the code; a concern that spans layers is several components, one per layer (`comment_threads` in the domain, `comment_store` in infrastructure).
- **Group what belongs together.** A component that grows several files becomes a directory: its entry file (`mod.rs`, `index.ts`, `__init__.py`) carries the module doc and re-exports what others use, and the files inside take its layer. Group files of one layer that import each other and serve one job; one component per small file is as unreadable as three named after layers. Aim for a handful to a couple of dozen components per container.
- **Go through the front door.** Import a sibling component from its entry file, never from a file inside it, and keep what others don't use private to the component.
- **New code joins the component whose job it serves.** Start a new component only for a new job, and give it its doc and layer when you create it.

**Decisions live in the module's doc comment.** A node's module doc comment (`//!` in Rust, the file's leading `/** */` in TypeScript, a module docstring in Python) has two parts:

- **The first paragraph is its description:** what the module is, in one card line of at most 200 characters.
- **Everything after it is its decisions,** as markdown: when you make a design decision a later reader needs (why this shape, what was rejected, what must stay true), add it there with its reason.

No separate ADR files; the doc comment travels with the code it explains. A decision spanning several modules goes on the lowest node containing them all; the ones below inherit it. The system's description and decisions are the root `ARCHITECTURE.md`, first paragraph and the rest.

## 4. Verify

1. `get_test_radius` (no `files` = this session's edits) names the tests your change reaches and, when configured, the exact command. Run those, never the whole suite.
2. Have the runner write JUnit XML: vitest `--reporter=junit --outputFile=…`, pytest `--junitxml=…`, `dotnet test --logger junit`, cargo through nextest's JUnit output or a cargo-to-JUnit converter.
3. `ingest_test_report {path}`, once per report file. That is what turns claims proven or failing; nothing else does.

A later edit to a test or to the code it exercises makes its verdict **stale** until the next ingested run. Before saying you are done, your claims should be proven and current.

## 5. Probe what you doubt

A green test proves only what it checks. For a claim that matters, `open_probe {test}` gives you an isolated worktree and the code to break. Break it the way the claim says it can't break, run that one test there, and `close_probe` with how many breaks you tried and the ones the test did not catch. Run probes yourself, never through a subagent. A surviving break is a finding: strengthen the test until it catches it, then probe again. `ingest_mutation_report` takes cargo-mutants or Stryker output the same way.

## Other sessions

Several agents may be working in this repository at once. Nothing locks. Don't revert uncommitted changes you didn't make, and don't delete todo tests you didn't write unless the user asks.

## Comments

People and other agents leave comment threads on nodes: questions, requests, decisions to respect. `orient`, `locate` and the overlay on a file you read list the open threads on its node and the nodes above it; `comments` lists them all.

- An open thread is an ask. If your task touches it, answer it with `comment {thread, text}`.
- Close a thread only when its ask is done: `comment {thread, text, addressed: true}`, saying in `text` what you did and where. A reply never closes a thread on its own, and an ask you only partly did stays open.
- To ask the user or another agent something you cannot settle yourself, open a thread: `comment {on, text}` with `on` a file, node id or node name. Keep it to one ask; don't narrate your work there.

## A project from before claims lived in tests

Older projects kept an authored model in `.scryer/model.scry`; nothing reads it now. Moving it into the code takes two passes, and orient says which one is due.

**1. The mechanical pass: `scryer-mcp migrate`** (or Migrate in the app). Ask the developer before running it and have their work committed first, since it writes into test files. By rule it writes each claim linked to a test as that test's description (the one closest to the test's name, and only when it fits the name), each descriptive statement as a module doc, each untested claim as a todo test in an existing test project, and container names and styles into `.scryer/structure.toml`. It leaves `.scryer/migration.md`: a checklist of every write to confirm and every claim it could not place, with why.

**2. The review: finish `.scryer/migration.md`.** When the developer asks you to finish the migration, or orient names the checklist, work through it against today's code. The old model can be weeks behind, and its links were often to a related test rather than the one that checks the claim.

- Go file by file: read a test file once and settle every item that names it.
- **Written as a test description:** keep it when the test checks it and it is true. Otherwise reword it, move it to the test that does check it, or delete the summary so the test's name speaks; a test name that already says it well beats an old sentence.
- **Written as a todo test:** keep it when it is still wanted; delete it when the code moved on.
- **Not written:** describe the test that checks it, write a todo test where it belongs, or drop it when it is no longer true. Several claims on one test usually mean one belongs there and the rest belong to other tests or none.
- Settle by reading the code what the code can settle. Ask the developer only what it cannot, such as whether an unbuilt claim is still wanted, and ask in batches.
- Keep each claim in the EARS shape above; an old claim in the imperative (`clunk it`) becomes a response (`Clunks it`).
- Tick each item (`- [x]`) as you settle it, so the review survives across sessions; a long checklist is several sessions' work. Run what `get_test_radius` names for the tests you changed and ingest the report.

When no item is open, delete `.scryer/migration.md` and the old model's files (`model.scry`, `planned.scry`, `model.baseline.scry`, `history.jsonl`).

## Without the MCP server

The same answers come from the CLI: `scryer-mcp status` (counts and what needs attention), `scryer-mcp check` (structural and test gates, non-zero on findings), `scryer-mcp lint` (claim wording), `scryer-mcp export` (the derived tree as JSON).

## Don't

- Don't write a claim anywhere but in a test.
- Don't mark anything proven by hand or edit files under `.scryer/` other than `structure.toml` when the user asks. Comment threads are written through `comment`, never by hand.
- Don't loosen a style to clear a violation.
- Don't delete another session's todo tests.
