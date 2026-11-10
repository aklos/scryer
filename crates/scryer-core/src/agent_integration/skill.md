---
name: scryer
description: How to plan, build and verify code in a project Scryer reads. Scryer derives the architecture from the code and its claims from behaviour tests, so a plan is a set of todo tests and "done" is a passing, ingested test report. Use for any coding task here — new behaviour, a fix, a refactor, a move between modules — and whenever orient, locate or a [scryer] note mentions claims, layers or other sessions.
---

<!-- Written by `scryer-mcp setup on`; `setup off` removes it. -->

# Working in a Scryer project

Scryer reads this repository and derives its C4 tree: containers from packaging, components from modules and directories, links from imports. Claims come from behaviour tests: **a test's description is a claim** about the code it tests, and its status (proven, unproven, outdated, failing, todo) comes from test reports. You never edit a model. There is none. You write code and tests; Scryer reads them.

The loop: **orient → write the claims as todo tests → build → run what the change reaches → ingest the report → probe what you doubt.**

## 1. Orient

Before touching code, call `orient` with the task in 3–5 words and the files you expect to touch. The task names your session on the developer's Sessions page, where they follow it as it works: `queue offline saves`, not a sentence. Per file you get:

- where it sits (`system / container / component`), its **layer** with what that layer may import, and any style violation already charged to the file,
- the module description and the claims that already speak for it, with status,
- `stubs`: the test file where a new claim for this code goes.
- `untested`, when no running test reaches the file: write the claim for what you change as a todo test before touching it.

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

```go
// When a capture settles, posts it to the ledger.
func TestPostsSettledCapture(t *testing.T) { t.Skip("todo") }
```

```java
@Test @Disabled @DisplayName("When a capture settles, posts it to the ledger")
void postsSettledCapture() {}
```

```ruby
it "When a capture settles, posts it to the ledger"   # RSpec: no block is pending
```

```php
/** @testdox When a capture settles, posts it to the ledger */
public function testPostsSettledCapture(): void { $this->markTestIncomplete('todo'); }
```

**Level.** The test's level is the claim's level. A unit test claims for its component, an integration test (at a deployable's boundary) for its container, an e2e test for the whole system. Write each claim at the level it speaks for; `.scryer/structure.toml` `[levels]` says which paths are which.

## 3. Build

Make the todos pass and turn each into a real test: drop `#[ignore]` / `.todo` / `skip`, keep the sentence. If the behaviour you built differs from the sentence, change the sentence; it must stay true.

**Where code goes.** A container may declare an architecture style in `.scryer/structure.toml` (hexagonal, feature-sliced, core-shell, library, …) with layers. Respect what the file's layer may import. When a file breaks its container's style, **fix it by moving code** to the layer it belongs in. Never change the style, the layers or `structure.toml` to make a finding go away, unless the user asks. A container declared `tooling = true` (demos, harnesses, dev apps) is not held to style rules; orient says so. One declared `as_layer = { "src/Shared" = "domain" }` uses that container as its domain: its imports of it follow its own matrix, and the other container is held to the domain's rules.

**Relationships the code cannot show.** Imports are read; a dependency the imports don't carry (a process you spawn, a server you call over HTTP, a person or external using the system) goes in `.scryer/structure.toml` `[relationships]`, with a label saying what the relationship is. Either end is any node: the system, a person or an external by its key, a container by its key, a component by its path, a symbol as `file#Name`. Declare it on the lowest nodes that have it (`"src-tauri/src/shell/preview.rs" = [{ to = "preview", label = "runs with node as its preview sidecar" }]`). Scryer implies it on every ancestor of both ends, so never declare it again higher up. C4 never relates a node to its own parent or child. Change relationships only for the behaviour you are building, or when the user asks.

**How to organise it.** Scryer draws each module or directory as a component, so how you organise code is what the reader sees:

- **A component is one job behind one entry file.** Name it for what it does (`billing`, `claim_text`, `session_store`), never for its layer (`domain`, `services`, `utils`). A layer is where a component sits, not a component.
- **A component sits in one layer.** In a styled container, end the first paragraph of its module doc with its layer: `//! Which agent to run, and how to spawn it. [domain]`. A folder named for a layer (`domain/`, `features/`) is that layer, and each folder inside it is a component on it; don't pile a layer's code loose in it as one component. A concern that spans layers is several components, one per layer (`comment_threads` in the domain, `comment_store` in infrastructure).
- **Group what belongs together.** A component that grows several files becomes a directory: its entry file (`mod.rs`, `index.ts`, `__init__.py`, Go's `doc.go`, Java's `package-info.java`, or in a language without one the file named after the folder, `Cargo/Cargo.cs`) carries the module doc and re-exports what others use, and the files inside take its layer. Group files of one layer that import each other and serve one job; one component per small file is as unreadable as three named after layers. Aim for a handful to a couple of dozen components per container.
- **Go through the front door.** Import a sibling component from its entry file, never from a file inside it, and keep what others don't use private to the component.
- **New code joins the component whose job it serves.** Start a new component only for a new job, and give it its doc and layer when you create it.

**Decisions live in the module's doc comment.** A node's module doc comment (`//!` in Rust, the file's leading `/** */` in TypeScript, a module docstring in Python, the `ns` docstring in Clojure, and in C#, Java, Kotlin, Go, C/C++, PHP and Ruby the comment the file opens with, before any code, apart from a licence and not sitting on a declaration) has two parts:

- **The first paragraph is its description:** what the module is, in one card line of at most 200 characters.
- **Everything after it is its decisions,** as markdown: when you make a design decision a later reader needs (why this shape, what was rejected, what must stay true), add it there with its reason.

No separate ADR files; the doc comment travels with the code it explains. A decision spanning several modules goes on the lowest node containing them all; the ones below inherit it. The system's description and decisions are the root `ARCHITECTURE.md`, first paragraph and the rest.

**A decision about one behaviour goes on its test.** A test's doc splits the same way: the first paragraph is the claim, everything after it is why the behaviour is this one (`If the token has expired, then rejects with 401.`, then `Not refreshed: a refresh would hide a revoked session.`). Where the claim is a string or attribute (`it("…")`, `@DisplayName`, TestDox), the comment right above the test is its decisions; in .NET, its `<remarks>`. Keep the claim itself one sentence.

## 4. Verify

1. `get_test_radius` (no `files` = this session's edits) names the tests your change reaches and, when configured, the exact command per runner. Run those, never the whole suite. In a project with several runners, set one command per runner: `get_test_radius {command: "[*.rs] cargo nextest run …"}` and `{command: "[*.ts *.tsx] npx vitest run {files} …"}`; a line without `[globs]` runs every file no other line claims.
2. Have the runner write JUnit XML: vitest `--reporter=junit --outputFile=…`, pytest `--junitxml=…`, `dotnet test --logger junit`, cargo through nextest's JUnit output or a cargo-to-JUnit converter.
3. `ingest_test_report {path}`, once per report file. That is what turns claims proven or failing; nothing else does.

A test no report has covered yet is **unproven**; a later edit to a test or to the code it exercises makes its verdict **outdated** until the next ingested run. Before saying you are done, your claims should be proven and current.

**Which lines each claim runs.** In a Rust project, wrap the radius command once in a while: `scryer-mcp coverage run -- cargo nextest run …` runs it with per-test coverage and prints an LCOV path; `ingest_test_report` it like a JUnit file. `locate` and `orient` then list a file's lines no claim runs: behaviour nothing states. Before relying on such code, write the claim it should meet; code nothing needs can go. They also name concern gaps: lines only claims tagged `[auth]` (or another concern) from elsewhere run, so the code serves that concern where nothing says so; write the tagged claim beside the code.

## 5. Probe what you doubt

A green test proves only what it checks. For a claim that matters, `open_probe {test}` gives you an isolated worktree and the code to break. Break it the way the claim says it can't break, run that one test there, and `close_probe` with how many breaks you tried and the ones the test did not catch. Run probes yourself, never through a subagent. A surviving break is a finding: strengthen the test until it catches it, then probe again. `ingest_mutation_report` takes cargo-mutants or Stryker output the same way.

## Other sessions

Several agents may be working in this repository at once. Nothing locks. Don't revert uncommitted changes you didn't make, and don't delete todo tests you didn't write unless the user asks.

When another session has work at stake, scryer names it with how to reach it: `orient`, `locate` and the overlay on a file you read list its uncommitted edits and the todo tests it owns there, and `ingest_test_report` names who last verified a claim your run turned red. To change that work, ask its session first, the way the line says: message it by name in Claude Code, `codex queue` in Codex, or, when it is not running, a comment thread on the code.

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
