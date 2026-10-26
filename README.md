<div align="center">

  <h1>
    <img width="50" src="public/logo.png" alt="Scryer logo" align="absmiddle" />
    &nbsp;scryer
  </h1>

  <p>
    <b>Spec-driven development where the spec is your tests.</b>
    <br />
    Coding agents write each behaviour as a test before they build it, and keep the code inside its declared architecture. Deterministic checks confirm both. What's left is a live, verified map of what your codebase does.
    <br />
    <br />
    <a href="#how-it-works">How it works</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#getting-started">Getting started</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#writing-claims">Writing claims</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#mcp-server">MCP server</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#building-from-source">Building from source</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="https://aklos.github.io/scryer/">Docs</a>
  </p>

</div>

> [!IMPORTANT]
> Scryer is in **alpha**. Expect rough edges and breaking changes before 1.0. Bug reports and feedback are very welcome in [issues](https://github.com/aklos/scryer/issues).

<br/>

<p align="center">
<video src="TODO-new-demo" width="100%" autoplay loop muted></video>
</p>

Coding agents write faster than you can read. After a week, or a weekend of three agents running in parallel, nobody can say with confidence what is implemented, what was only planned, and what quietly broke. Ask the agent and it guesses from whatever code it has open.

Spec-driven development answers that with a document written up front: a spec, a plan, a task list. The document is prose, so nothing checks it, and it starts drifting from the code as soon as the work begins.

In scryer the spec is the test suite. An agent plans a change by writing each behaviour it will add as a todo test, one sentence per test, and builds until those tests pass. Its imports are checked against the architecture style each container declares. A green test it doubts gets probed: the code under it is broken on purpose, to see whether the test notices. Every one of those checks is deterministic: a test run, the import graph, a probe's red or green. No model grades another model's work.

What comes out is the documentation. Scryer derives a [C4](https://c4model.com/) map of the repository: containers from packaging, components from modules, links from imports, claims from test descriptions, and each claim's status from real test runs. You read a column of sentences instead of an essay, see which hold right now, and see what changed since you last looked. Nobody writes the map, so it cannot drift.

Inside a session scryer drives and never blocks: MCP tools, session hooks and a skill steer Claude Code, Codex or Copilot CLI through the loop. At the gate it enforces: `scryer-mcp check` in CI fails on failing claims, architecture violations and malformed claim text.

## How it works

### The spec is the tests

**The claim is the test.** A behaviour test's description, the `///` doc comment on a Rust test, the string in `it("…")`, a pytest docstring, a .NET `DisplayName`, is an EARS claim about the code it tests: *When a save fails, keeps the draft.* Scryer lifts those sentences and lints them live. One test, one claim.

**The level is the test level.** An end-to-end test claims for the system. An integration test at a deployable's boundary claims for its container. A unit test claims for its component. The map is the test pyramid with the C4 vocabulary on it.

### Agents write the spec first

**A plan is a set of todo tests.** Before building, an agent calls `orient` with its task and files, and writes each claim it will satisfy as a todo test in the file orient names. You review the plan as sentences, and they turn proven as the work lands. A todo is also how you record behaviour that exists but has no test body yet.

**Run what the change reaches.** `get_test_radius` names the tests a change reaches by import, as the exact command to run. The agent runs them and hands scryer the JUnit report.

**Several agents, one spec.** Each session's touched files and todo stubs are recorded. An agent orienting on a file sees that another session edited it, that a todo there belongs to someone else, or that their test radius overlaps. Awareness, not locks: coordination happens through the repo.

**Decisions live with the code.** A node's description is its module's doc comment, and that is where an architectural decision goes: what was decided and why, beside the code it governs, read by every agent that orients there. No separate ADR files to drift.

**Comments are issues on the code.** You or an agent can open a comment thread on any node, from the rail beside its page or with the `comment` tool: a question, a request, a decision to respect. An agent working on that code sees the open threads in `orient` and when it reads a file there, replies, and closes a thread only by explicitly marking it addressed, saying what it did. Threads are committed under `.scryer/comments/`, one file per event, so they travel with the branch, merge without conflicts, and show from every worktree at once.

**Nothing talks at turn end.** Session hooks record what an agent touched and put the claims for a file in front of it when it reads that file. They emit no errors, no summaries, no gates.

### Deterministic checks

**Status comes from test runs.** A claim is proven when its test passed and neither the test nor the code it exercises changed since; failing when it is red; todo when its test is a todo; stale when it has no current verdict, because something changed since its last run or it has never been run.

**Probes ask whether a green test would notice.** `open_probe` lets an agent break the code a passing test exercises, in an isolated worktree, and run the test expecting red. A break that survives is a finding on the claim. Mutation reports from cargo-mutants or Stryker feed the same mark without an agent in the loop.

**Styles are structural claims.** A container declares its architecture style in `.scryer/structure.toml` (hexagonal, library, feature-sliced, …) and the layers it has. The import graph is checked against it, and a file being edited shows the agent which layer it is on and what that layer may import. A violation is fixed by moving the code or, when the declared style was a guess, by declaring the shape the code actually has; never by loosening a rule to let one violation through. A container with no style yet is a decision still to make, not a violation. Tooling containers (`tooling = true`) are part of the codebase but held to no style.

**The gate.** `scryer-mcp check` fails on failing claims, architecture violations and claim text that breaks the grammar. Stale claims and surviving mutants are notes by default and failures with `--fail-on-stale` and `--fail-on-mutants`.

### Reading the result

**The map.** The desktop app shows the derived C4 map level by level, a page per node with its claims and their status, and what changed since you last looked: newly proven, still todo, newly stale or failing, and which components more than one session touched. Health counts what needs attention: todo, stale and failing claims, components with no claims, files no test reaches, surviving mutants, architecture violations.

**Concerns cut across.** A trailing `[persistence]` or `[auth]` on a claim tags it with a concern. The concern lens dims everything else, so you can read down a component or across a concern.

## Getting started

Download the latest release for your platform from the [releases page](https://github.com/aklos/scryer/releases).

1. Open your project in the app. Scryer derives the tree from your packaging and tests on the spot; a project with no behaviour tests shows its containers and components with nothing proven yet.
2. Flip **Agent integration** on in settings (or run `scryer-mcp setup on`). This writes the MCP entry and session hooks for whichever of Claude Code, Codex and Copilot CLI you have, and nothing else.
3. Add `.scryer/structure.toml` to name the system, its people and external systems and how they relate, which directories hold e2e and integration tests, and each container's style:

   ```toml
   [system]
   name = "aperture-pay"
   persons = ["cardholder", "merchant"]
   externals = ["acquiring bank", "card networks", "payments-db"]

   [relationships]
   cardholder = ["aperture-pay"]
   merchant = [{ to = "aperture-pay", label = "integrates" }]
   "aperture-pay" = ["acquiring bank", "card networks"]

   [levels]
   e2e = ["tests/e2e/**"]
   integration = ["crates/*/tests/**"]

   [containers.ledger]
   style = "hexagonal"
   uses = ["payments-db"]
   ```

4. Give your agent a task. It orients, writes the claims it will satisfy as todo tests, builds, runs the radius, and ingests the report. Open scryer's landing page to see what changed since you last looked: newly proven, still todo, newly stale, and which components more than one session touched.
5. Run `scryer-mcp check` in CI to fail on failing claims, architecture violations, or lint errors in claim text.

## Writing claims

EARS, subject dropped, condition first, one response per claim:

| Pattern | Shape | Example |
|---|---|---|
| Ubiquitous | `<verb> <object>` | Persists the ledger on every post |
| Event | `When <trigger>, <response>` | When a refund exceeds the captured amount, rejects it |
| State | `While <state>, <response>` | While settlement is unconfirmed, holds funds in escrow |
| Unwanted | `If <condition>, then <response>` | If the bank times out, then retries once |

A trailing `[tag]` names a concern. A todo test (`it.todo`, `#[ignore]`, `@pytest.mark.skip`, `[Fact(Skip = …)]`) is a todo claim. A `[manual]` tag keeps a test out of the model.

## MCP server

The MCP server lets AI agents read the tree derived from your code and keep the test loop; it writes no model. It ships bundled with the desktop app.

### Setup

One switch per project, from the app (settings → Agent integration) or the command line:

```
scryer-mcp setup        # what is installed, per tool, per file
scryer-mcp setup on     # write every entry for each detected agent CLI
scryer-mcp setup off    # remove exactly scryer's entries, nothing else
```

`on` writes, for each agent CLI on PATH:

- **Claude Code** — the scryer server in `.mcp.json`; auto-approve, session hooks and the status line in `.claude/settings.local.json`
- **Codex** — the scryer server in `.codex/config.toml`; session hooks in `.codex/hooks.json`
- **Copilot CLI** — the scryer server in `.mcp.json` (the file Claude Code reads); session hooks in `.github/hooks/scryer.json`

and `.scryer/.gitignore`, so only what a teammate needs is committed. Both commands are idempotent. Everything else in those files is left alone, and a status line of your own is never replaced. An entry from an earlier scryer, or one naming a binary that has moved, reads as off with a note; `on` rewrites it.

### Manual setup

If you prefer to configure MCP manually, add Scryer to your project config:

**Claude Code** (`.mcp.json` in project root):

```json
{
  "mcpServers": {
    "scryer": {
      "type": "stdio",
      "command": "/path/to/scryer-mcp"
    }
  }
}
```

**Codex** (`.codex/config.toml` in project root):

```toml
[mcp_servers.scryer]
command = "/path/to/scryer-mcp"
```

For Claude Code, you can also auto-approve Scryer's tools so the agent doesn't prompt for every call. This is safe: they read your code and tests and write only scryer's own caches and records under `.scryer/` — never your source, shell, or network. The app can set this up for you, or add the server-wide entry manually to `.claude/settings.local.json`:

```json
{
  "permissions": {
    "allow": [
      "mcp__scryer"
    ]
  }
}
```

### What the MCP server provides

The server reads the C4 tree derived from your code — containers from packaging, components from modules, claims from behaviour-test descriptions — and keeps the test loop. It writes no model: a claim is a test.

- `orient` — the front door for a coding task: per file its node chain, layer, module description, claims with status, and the test file where new todo stubs go; per task the nodes it matches. Capped at about 4k characters.
- `locate` — one file in full: every claim that speaks for it with status, test level and JUnit name, and where its stubs go.
- `read_model` / `search_model` — the derived tree, as an overview, a subtree, or ranked text search.
- `get_test_radius` — which tests a change reaches by import, at unit and integration level (e2e too with `radius = "all"` in `.scryer/structure.toml`), as the exact command to run.
- `ingest_test_report` — record a run's JUnit XML against the claims; a later edit to the test or the code it exercises makes the verdict stale.
- `ingest_mutation_report` — record a cargo-mutants or Stryker run; surviving mutants mark the code they landed in.
- `open_probe` / `close_probe` — break the code a passing test exercises in an isolated worktree and see whether the test notices; a surviving break is a finding.
- `comments` / `comment` — read the comment threads on a node, reply to one, close it as addressed, or open one to ask the user something.

### The scryer skill

`scryer-mcp setup on` also installs a `scryer` skill: `.claude/skills/scryer/SKILL.md` for Claude Code and Copilot, `.agents/skills/scryer/SKILL.md` for Codex. Agents load it only when a task calls for it. It carries the whole loop: orient, write the claims as todo tests, build, run what the change reaches, ingest the report, and probe what you doubt. It also covers EARS with examples per language, how to fix a structural violation by moving code, and how to work beside other live sessions. `setup off` removes it; a `SKILL.md` of your own in that place is left alone.

### Suggested CLAUDE.md

The server's own instructions, worth keeping in the project's CLAUDE.md (or AGENTS.md) too:

> Scryer reads this project's architecture from its code: containers from packaging, components from modules, claims from behaviour tests. A behaviour test's description is an EARS claim about the code it tests, subject dropped, condition first: `<verb> <object>` when always true; `When <trigger>, <response>`; `While <state>, <response>`; `If <unwanted condition>, then <response>`. One pattern and one response per test; a cross-cutting concern is a trailing `[tag]`. The test's level is the claim's level: an e2e test claims for the whole system, an integration test at a deployable's boundary for its container, a unit test for its component — write a claim at the level it speaks for. Before building, `orient {task, files}` and write each claim you will satisfy as a todo test in the file orient names; a todo test is work to do. Run only what `get_test_radius` names, then `ingest_test_report`. A surviving probe (`open_probe`) is a finding: strengthen the test. An open comment thread on code you work on is an ask: reply with `comment`, and mark it addressed only when it is done. Every tool takes an optional absolute `project`, defaulting to the working directory.

## Tech

Rust core (`scryer-core`, `scryer-extract`, `scryer-mcp`), Tauri 2 shell, React frontend. Import resolution for Rust, TypeScript/JavaScript, Python, Go and Clojure; name-matching heuristics for Java, Ruby, C/C++, C#, PHP. Test lifting for Rust, Vitest/Jest, pytest and .NET.

## Building from source

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 20+
- [pnpm](https://pnpm.io/)
- System dependencies for [Tauri 2](https://v2.tauri.app/start/prerequisites/)

If you use Nix, `shell.nix` provides everything:

```bash
nix-shell
```

### Build & develop

```bash
pnpm install          # Install dependencies
pnpm tauri dev        # Run full app (Tauri + Vite on :1420)
pnpm dev              # Run frontend only
pnpm tauri build      # Production build
```

## License

Scryer is [Fair Source](https://fair.io/) software under the [Functional Source License (FSL-1.1-MIT)](LICENSE). You can use it, view the source, and contribute. You just can't build a competitor with it. The license converts to MIT after two years.
