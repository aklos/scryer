<div align="center">

  <h1>
    <img width="50" src="public/logo.png" alt="Scryer logo" align="absmiddle" />
    &nbsp;scryer
  </h1>

  <p>
    <b>Model-driven development for coding agents.</b>
    <br />
    A shared model you and your agent plan from. The model leads; the code follows.
    <br />
    <br />
    <a href="#features">Features</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#getting-started">Getting started</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#the-model-as-a-plan">The model as a plan</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#mcp-server">MCP server</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#building-from-source">Building from source</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="https://aklos.github.io/scryer/">Docs</a>
  </p>

</div>

> [!IMPORTANT]
> Scryer is in **alpha**. Expect rough edges, breaking changes between releases, and model-format migrations before 1.0. Bug reports and feedback are very welcome in [issues](https://github.com/aklos/scryer/issues).

<br/>

<p align="center">
<video src="https://github.com/user-attachments/assets/a05fd23d-a32c-4f06-b1b0-ff9419a0a6f9" width="100%" autoplay loop muted></video>
</p>

Coding agents write faster than you can review. You end up shipping code you don't fully understand, and what you meant drifts from what got built. Scryer keeps a model next to your code: a graph of what each part of the system is responsible for, mapped to the source lines that implement it and to the tests that back each claim. Use it to see how the code matches your intent, and to plan changes against that intent before the agent writes them.

The model describes what each part is responsible for — not its class-by-class structure — in short, language-independent claims on an opinionated [C4](https://c4model.com/)-style hierarchy. It isn't UML, and it isn't your code redrawn as boxes. You plan a change in the model first; the agent reads it over MCP and builds code to match, so the model stays ahead of the code rather than lagging behind it. Underneath, a deterministic observability layer (no LLM) reports what's built versus planned, source and test coverage, and drift in both directions.

Works with <b>Claude Code</b>, <b>Codex</b> and <b>GitHub Copilot CLI</b> out of the box. Any agent that supports [MCP](https://modelcontextprotocol.io/) can read and write the model.

## Features

- **Wiki-style node pages** — Every node, down to individual symbols, gets a page that leads with whatever helps you plan changes to that kind of thing: responsibilities for handlers and services (each mapped to source lines), properties for data types, a live rendered preview for visual components. Source code shows as reference, with structured metadata — kind, technology, status, connections — surfaced inline on the page. Edit any section in place.
- **Model tree** — An IDE-style explorer for defining the model: create, rename, move, group, and delete nodes. Groups are folders. Rolled-up plan and drift marks show per row, so you see the model's health at a glance.
- **Responsibilities & directives** — Each node states what it's responsible for (responsibilities) and, optionally, how the agent should implement it (directives, e.g. "authenticate with JWTs"). Responsibilities are language-independent and survive a rewrite. Claims are written in [EARS](https://alistairmavin.com/ears/) grammar — condition first ("When a save fails, …"), verb-led response last — so a testable claim already names the trigger to arrange and the response to assert, and the editor lints statements live as you write them.
- **Test backing** — Attaching a test is part of implementing a claim, not a follow-up chore. Every claim row carries a test lane: a filled flask means a test is attached, a ghost outline means testable but bare, and health counts `untested` claims so the gap stays visible. Scryer records attachment only — it never runs your tests.
- **Concerns** — A cross-cutting third axis over responsibilities: tag claims with a concern slug (`auth`, `persistence`, `idempotency`, …) and scan the model through a concern lens in the tree and on the map. Core domain flow stays untagged.
- **The plan** — Scryer holds a committed model (what the code satisfies) and a planned draft (what you and the agent edit). Their difference is the plan: the model→code work queue, shown as add/move/delete/reword marks until the agent implements them and they fold into the committed model. Parallel workstreams can be filed as named changes, each folding and closing on its own.
- **Live component previews** — Visual components (React/TSX) render deterministically through a per-project Vite dev server, with no agent and no per-component build.
- **Observability layer** — An always-on, deterministic health report over the model↔code relationship (no LLM): plan and drift rollup, source-anchor and test-backing coverage, disconnected nodes, and an import-graph audit of declared links against actual imports. The import graph resolves real declared imports for Rust, TypeScript/JavaScript (including tsconfig path aliases), Python, and Go; Java, Ruby, C/C++, C#, and PHP are covered by a conservative name-matching heuristic, and the health report says which tier applies so the audit's verdict is calibrated. It tells you where work is needed before you read a single page.
- **Drift detection & sync** — Scryer tracks when source files change relative to the model. When code diverges, the agent reads the changed files and reconciles the model, adopting new behavior or flagging stale claims.
- **Source mapping** — Responsibilities and nodes link to files and line ranges. Click to open in your editor.
- **Build from a codebase** — Point an agent at a project and it scans the code to populate the model.
- **Diagram view** — A diagram renders the model one level at a time for spatial navigation. Drag cards where you want them — placements persist, and auto-layout manages the rest.
- **MCP server** — Agents connect to read, modify, and build from the model in real time.
- **AI tool setup** — Detects Claude Code, Codex and Copilot CLI and writes MCP config and auto-approve permissions. Session hooks put the model in front of the agent as it works, per project and per tool. Optionally installs a Claude Code statusLine that reports the model's pending work and drift even while Scryer is closed.

## Getting started

Download the latest release for your platform from the [releases page](https://github.com/aklos/scryer/releases).

### Typical workflow

1. Link your project directory in the app and enable AI tool integration when prompted (or run `scryer-mcp init`).
2. Tell your agent: *"Use scryer to model this project's architecture."* It scans the code and the model fills in, with nodes appearing in the tree and on their pages in real time.
3. Review the model: read the node pages, rename, regroup, restructure, and refine responsibilities.
4. To make a change, plan it in the model first: add or edit the responsibilities and links it implies. These land in the plan as pending work.
5. Tell the agent to implement the plan. It builds each piece and marks nodes implemented as it goes, folding the plan into the committed model.
6. Check the model's health any time. `get_health` reports the plan and drift rollup, source-anchor coverage, and where code and model have parted ways.

As you work on code, Scryer detects when source files drift from the model. Trigger a drift check to have the agent reconcile: adopting new behavior into the model, or flagging where the code regressed from what the model claims.

## The model as a plan

Scryer keeps two layers on disk in `.scryer/`:

- **`model.scry`** — the committed model, the source of truth: what the code is believed to satisfy.
- **`planned.scry`** — the draft you and the agent edit on the canvas.

The difference between them is the **plan**: the outstanding model→code work. When you add a responsibility or a node, it shows in the plan as an `added` mark in the tree. When the agent writes the code, `mark_implemented` folds that work into the committed model. Drift works the other way: when code changes, the agent reconciles undescribed behavior back into the model.

Plan work can be filed into **named changes**: a session opens a change with `set_change {rationale}` and its plan writes tag to it automatically, so parallel sessions (or you, on the canvas) stay separable. When a change's last entry folds in, it closes and its rationale lands in the history log.

This is how the model stays ahead of the code: intent is captured as a plan before the code exists, and the committed model only ever reflects what's actually been built.

## Agent support

Scryer works with **Claude Code**, **Codex** and **GitHub Copilot CLI**.

- **MCP** (Model Context Protocol) — how agents read and write architecture models. Required for any agent integration.
- **CLI spawning** — how Scryer launches agents for automated model builds and drift sync. Claude Code is spawned via `claude -p` (uses your subscription), Codex via `codex exec` (uses your API key). Both get the Scryer MCP server attached automatically.
- **ACP** (Agent Client Protocol) — Copilot CLI serves ACP from its own binary (`copilot --acp`), so that's how Scryer launches it; any other agent with a `{name}-acp` adapter on PATH is launched the same way.
- **Session hooks** — an optional, per-project opt-in for all three: the model's status on session start, the claims governing a file as the agent works in it, and a one-time close check for claims it touched. Inert whenever the Scryer app isn't open on the project.

When an agent connects via MCP, Scryer captures its identity from the protocol handshake. When a build or sync is triggered, Scryer resolves that identity to a binary and launches it with the right flags.

**Copilot notes.** Copilot reads the same project `.mcp.json` Claude Code does, so one file serves both. Two things differ from the others: it reaches Scryer *only* through that file (its ACP mode doesn't accept a stdio MCP server over the protocol), and it loads a project's MCP servers and hooks only in a folder you've **trusted** — it asks the first time you open one. Until you do, it stays quiet about both.

Copilot fronts several model providers on one subscription, so its model list spans Claude, GPT and Gemini; pick one under Subagent settings. If you type a model name by hand there, check the spelling — run `copilot help config` for the current list. Copilot ignores an unrecognised model in ACP mode rather than reporting it, so a typo runs on its default model instead of failing.

## MCP server

The MCP server lets AI agents read and modify your architecture models. It ships bundled with the desktop app.

### Setup

Link a project directory in the app and click "Enable" on the prompt, or run `scryer-mcp init` from the command line. Both detect installed AI tools and write config:

- **Claude Code** — `.mcp.json` + tool auto-approve in `.claude/settings.local.json`
- **Codex** — `.codex/config.toml`
- **Copilot CLI** — `.mcp.json`, the same file Claude Code reads (Copilot also accepts a committed `.github/mcp.json`, if you'd rather keep it there)

Existing config files are preserved — only the `scryer` entry is added or updated.

Session hooks are a separate opt-in, per tool, from Subagent settings: `.claude/settings.local.json`, `.codex/hooks.json`, or `.github/hooks/scryer.json` for Copilot.

For Claude Code you can also install a **statusLine** (from the app, or `scryer-mcp init --statusline`): a one-line model status — pending work, drift scopes, and changes in flight — in every Claude Code session. It reads the model straight off disk, so it keeps reporting while Scryer is closed.

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

For Claude Code, you can also auto-approve Scryer's tools so the agent doesn't prompt for every call. This is safe: Scryer's tools only ever read or mutate the model under `.scryer/` — which is git-tracked and shown in Scryer's own diff — never your source, shell, or network. The app can set this up for you, or add the server-wide entry manually to `.claude/settings.local.json`:

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

**Reading & observability:**
- `orient` — one-call, task-scoped orientation: pass a task and/or files and get the governing nodes, their claims and binding directives, the scoped pending work and drift, the matching modeling rules, and a phase verdict. The front door for coding sessions.
- `locate` — reverse lookup from code into the model: a file (and optional symbol) resolves to the claims anchored there, the owning node chain, binding directives, and the surrounding scope's health.
- `read_model` — the model, or a scoped subtree, with responsibilities, links, and context. Auto-resolves the model linked to the current working directory.
- `search_model` / `query_model` — find nodes by text or by structure.
- `get_health` — deterministic observability: rolled-up plan and drift marks, vagrant/stale flags, source-anchor and test-backing coverage, disconnected nodes, and an import-graph audit of declared links.
- `get_pending` — the plan: model→code work not yet built.
- `get_drift` — boundary-owning nodes whose code changed since the last reconcile (cheap, deterministic — no LLM verdict).
- `get_rules` — the authoritative C4 modeling rules and workflow guidance.
- `read_codebase` — annotated project tree: deployable units, data stores, external services.
- `validate_model` — check the model against C4 rules.

**Authoring (writes the plan):**
- `set_change` — open (or resume) a named change so this session's plan writes tag to it, keeping parallel workstreams separable.
- `add_person` / `add_system` / `add_container` / `add_component` / `add_symbol` — mint nodes from plain responsibility statements.
- `add_group` / `update_group` / `delete_group` — group sibling nodes (a secondary packaging axis).
- `add_links` / `update_links` / `delete_links` — typed relationships between nodes.
- `update_source_map` — link nodes and responsibilities to files and line ranges, and claims to their backing tests.

**Building & reconciliation:**
- `fill_container` — fill in a whole container's subtree at once when extracting from existing code (generation pipeline).
- `mark_implemented` — fold implemented work from the plan into the committed model, anchoring claims and linking their tests in the same call.
- `flag_drift` / `reconcile_drift` — record undescribed behavior or stale claims, then advance the drift anchor.
- `update_nodes` / `delete_nodes` / `descope` / `move_nodes` / `move_responsibilities` — interactive edits and refinement (`descope` drops a node from the model while leaving its code in place).
- `set_model` / `set_node` / `set_groups` — generation-pipeline primitives for whole-model / whole-subtree / bulk-group writes.

## Drift detection & sync

Architecture models go stale as code changes. Scryer detects drift deterministically — no LLM — two ways: source-mapped nodes whose files changed since the last reconcile, and new files appearing in the project that the model doesn't cover yet.

When drift is detected:

1. A review surface and drift indicators flag the potentially drifted scopes — click through to the affected node pages.
2. Trigger a drift check to spawn your agent (Claude Code via `claude -p`, Codex via `codex exec`, Copilot via `copilot --acp`) with Scryer's MCP server attached. Drifted scopes are checked in parallel, and each agent gets the changed code embedded inline as evidence, so it judges divergence without re-reading the tree and updates the model only where code has actually diverged.
3. Undescribed behavior is proposed into the plan as a **vagrant** claim for you to adopt or reject; a **stale** claim is flagged on the committed model where the code regressed from what was claimed.
4. Model changes appear in the editor in real time. When every scope has been examined, the drift anchor advances so the same changes stop surfacing.

For Claude Code, the MCP server config is passed inline via `--mcp-config`. For Codex, the project must have MCP already configured (via `scryer-mcp init` or the app's setup flow), since Codex reads MCP config from `.codex/config.toml`.

## Tech

Scryer is a [Tauri](https://tauri.app/) desktop app. The UI is written in [React](https://react.dev/) with [TypeScript](https://www.typescriptlang.org/) — a two-pane workspace of a model tree and wiki-style node pages, with a secondary [ReactFlow](https://reactflow.dev/) diagram for spatial navigation. The backend is written in [Rust](https://www.rust-lang.org/): the core model, diff, drift, and health engines (`scryer-core`), the tree-sitter code-extraction and import-graph engine (`scryer-extract`), the MCP server (`scryer-mcp`), and ACP integration (`scryer-acp`). Live component previews run through a per-project [Vite](https://vite.dev/) dev server.

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
