<div align="center">

  <img width="100px" src="public/logo.png" alt="Scryer logo" />

  <h1>scryer</h1>

  <p>
    <b>MDD for AI agents.</b>
    <br />
    A shared planning substrate you and your AI agent build from — the model leads, the code follows.
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

<br/>

<p align="center">
<video src="https://github.com/user-attachments/assets/a67f5159-aac1-49b7-abba-dae11aad9499" width="100%" autoplay loop muted></video>
</p>

Coding agents write faster than you can read. You end up shipping code you don't fully understand — and what you meant drifts from what got built. Scryer keeps a living model alongside your code: a graph of what each part of the system is **accountable for**, mapped to the exact source lines that discharge it. One substrate, two jobs — **see** how the code measures up to your intent, and **plan** changes against that intent before the agent writes them.

The model sits at a **higher altitude than the code**. It captures what each part is accountable for — intent and meaning — not class-by-class structure. The code stays the source of truth for *how* the system works; the model is the source of truth for *what* it must do and *why*. **This isn't UML, and it isn't your code restated as boxes:** responsibilities are written to survive a rewrite in another language, because they describe intent, not implementation.

And the model **leads**. You plan a change in the model first; the agent reads it through MCP and builds the code to match, keeping the two mapped line-by-line — so the model stays a step ahead of the code instead of decaying behind it. Underneath runs a deterministic, always-on **observability layer** — no LLM — that reads how the code actually measures up: what's built versus still planned, source coverage, and drift in both directions, so you can see where intent and reality have parted ways.

Works with <b>Claude Code</b> and <b>Codex</b> out of the box. Any agent that supports [MCP](https://modelcontextprotocol.io/) can read and write the model. Agents that support [ACP](https://agentclientprotocol.com/get-started/introduction) can also be spawned by Scryer for automated builds and sync.

Opinionated [C4](https://c4model.com/) hierarchy (person, system, container, component, symbol), responsibilities with implementation directives, typed relationships, and source mapping.

## Features

- **Wiki-style node pages**
  - Every node — down to individual symbols — gets its own page. The page leads with the representation that actually helps you plan changes to *that* kind of thing.
  - **Handlers, services, process logic → responsibilities.** A list of what the node is accountable for, each mapped to the exact source lines that discharge it.
  - **Data types → properties.** The field shapes, not prose.
  - **Visual components → a live rendered preview.** The actual component, interactive, embedded on the page.
  - Source code is always shown as reference, and an infobox carries the structured metadata (kind, technology, connections as wiki-links, boundary globs). Edit any section in place — no modals.
- **Model tree**
  - An IDE-style explorer is the definition surface: create, rename, move, group, and delete systems, containers, components, and symbols. Groups are folders; nesting is native.
  - Rolled-up plan and drift marks show as a colored marker per row, so you see the health of the whole model at a glance.
- **Responsibilities & directives**
  - Each node states *what* it's accountable for (responsibilities) and, optionally, *how* the agent should implement it (directives — e.g. "authenticate with JWTs"; user-authored, not part of conformance). Responsibilities are language-independent: they survive a rewrite in a different language.
  - There's no stored "status" — where a claim stands is read live from two axes. The **plan** is its diff against the committed model (**added** / **reworded** / **moved** / **deleted** marks — work not yet built). The **drift flags** are the other axis: **vagrant** (the code does something the model doesn't describe — adopt it?) and **stale** (the code that backed a claim regressed or is gone — re-implement or drop?). Both surface as marks in the tree and on the page.
- **The plan (a two-layer model)**
  - Scryer holds a committed model (what the code is believed to satisfy) and a planned draft (what you and the agent are editing). Their difference is the **plan** — the model→code work queue. Changes show as add/move/delete/reword marks in the tree until the agent implements them and they fold into the committed model.
- **Live component previews**
  - Visual components (React/TSX today) render deterministically through a shared Vite dev server scoped to your project — no agent and no per-component build. When a render needs realistic data, the agent writes a fixture; when you want to redesign, prompt for variations, compare live renders, and accept the one you want.
- **Observability layer**
  - A deterministic, always-on health report over the model↔code relationship — no LLM. It rolls up the plan and drift marks across the tree, surfaces source-anchor coverage (which responsibilities map to code and which are blind spots), audits declared links against the actual import graph (both unbacked links and connections the code makes that the model doesn't declare), and flags vagrant (undescribed) and stale (regressed) claims. The import graph is parsed deterministically across Rust, TypeScript/JavaScript, Python, Go, Java, Ruby, C/C++, C#, and PHP. It tells you *where* work is needed before you read a single page.
- **Drift detection & sync**
  - Scryer tracks when source files change relative to the model — cheap and deterministic, the code→model half of the observability layer. When code has diverged, the agent reads the changed files and reconciles the model — adopting undescribed behavior or flagging stale claims.
- **Source mapping**
  - Responsibilities and nodes link to files and line ranges in your codebase. Click to open in your editor.
- **Build from a codebase**
  - Point an agent at a project and it scans the code to populate the model — containers, components, symbols, responsibilities, source mapping, and links. Per-node "fill" does the same for one node at a time.
- **Secondary diagram view**
  - A read-only diagram renders the same model one level at a time for spatial navigation. It's a way to browse the architecture, not the place you do the work.
- **MCP server**
  - AI agents connect to read, modify, and build from your architecture model in real time.
- **AI tool setup**
  - Detects Claude Code and Codex, writes MCP config and auto-approve permissions for your project.

## Getting started

Download the latest release for your platform from the [releases page](https://github.com/aklos/scryer/releases).

### Typical workflow

1. Link your project directory in the app and enable AI tool integration when prompted (or run `scryer-mcp init`).
2. Tell your AI agent: *"Use scryer to model this project's architecture."* The agent scans the code and the model fills in — nodes appear in the tree and on their pages in real time.
3. Review the model: read the node pages, rename, regroup, restructure, and refine responsibilities.
4. To make a change, **plan it in the model first** — add or edit the responsibilities and links it implies. These land in the plan as pending work, shown as change marks in the tree.
5. Tell the agent to implement the plan. It builds each piece and marks nodes implemented as it goes; the plan folds into the committed model.
6. Check the model's health any time — `get_health` reports the plan and drift rollup, source-anchor coverage, and where code and model have parted ways, so you can see what's built, what's a blind spot, and what still needs reconciling.

As you work on code, Scryer detects when source files drift from the model. Trigger a drift check to have the agent reconcile — adopting new behavior into the model or flagging where the code regressed from what the model claims.

## The model as a plan

Scryer keeps two layers on disk in `.scryer/`:

- **`model.scry`** — the committed model, the source of truth: what the code is believed to satisfy.
- **`planned.scry`** — the draft you and the agent edit on the canvas.

The difference between them is the **plan**: the outstanding model→code work. When you add a responsibility or a node, it shows in the plan as an `added` change mark in the tree. When the agent writes the code, `mark_implemented` folds that work from the plan into the committed model. Drift works the other way: when code changes, the agent reconciles undescribed behavior back into the model.

This is why the model can stay *ahead* of the code instead of decaying behind it — intent is captured as a plan before the code exists, and the committed model only ever reflects what's actually been built.

## Agent support

Scryer is built to work with **Claude Code** and **Codex** first.

- **MCP** (Model Context Protocol) — how agents read and write architecture models. Required for any agent integration.
- **CLI spawning** — how Scryer launches agents for automated builds, fills, and sync. Claude Code is spawned via `claude -p` (uses your subscription), Codex via `codex exec` (uses your API key). Both get the Scryer MCP server attached automatically.
- **ACP** (Agent Client Protocol) — for agents that implement the full ACP handshake (e.g. via [claude-agent-acp](https://github.com/zed-industries/claude-agent-acp)). Scryer falls back to ACP if a `{name}-acp` binary is found on PATH.

When an agent connects via MCP, Scryer captures its identity from the protocol handshake. When a build or sync is triggered, Scryer resolves that identity to a binary and launches it with the right flags. Claude Code and Codex are mapped automatically. For other agents, Scryer tries ACP conventions.

## MCP server

The MCP server lets AI agents read and modify your architecture models. It ships bundled with the desktop app.

### Setup

Link a project directory in the app and click "Enable" on the prompt, or run `scryer-mcp init` from the command line. Both detect installed AI tools and write config:

- **Claude Code** — `.mcp.json` + read tool auto-approve in `.claude/settings.local.json`
- **Codex** — `.codex/config.toml`

Existing config files are preserved — only the `scryer` entry is added or updated.

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

For Claude Code, you can also auto-approve Scryer's read tools so the agent doesn't prompt for every read. The app can set this up for you, or add them manually to `.claude/settings.local.json`:

```json
{
  "permissions": {
    "allow": [
      "mcp__scryer__read_model",
      "mcp__scryer__search_model",
      "mcp__scryer__query_model",
      "mcp__scryer__get_health",
      "mcp__scryer__get_drift",
      "mcp__scryer__get_pending",
      "mcp__scryer__get_rules",
      "mcp__scryer__read_codebase",
      "mcp__scryer__validate_model"
    ]
  }
}
```

### What the MCP server provides

**Reading & observability:**
- `read_model` — the model, or a scoped subtree, with responsibilities, links, and context. Auto-resolves the model linked to the current working directory.
- `search_model` / `query_model` — find nodes by text or by structure.
- `get_health` — deterministic observability: rolled-up plan and drift marks, vagrant/stale flags, source-anchor coverage, and an import-graph audit of declared links.
- `get_pending` — the plan: model→code work not yet built.
- `get_drift` — boundary-owning nodes whose code changed since the last reconcile (cheap, deterministic — no LLM verdict).
- `get_rules` — the authoritative C4 modeling rules and workflow guidance.
- `read_codebase` — annotated project tree: deployable units, data stores, external services.
- `validate_model` — check the model against C4 rules.

**Authoring (writes the plan):**
- `add_person` / `add_system` / `add_container` / `add_component` / `add_symbol` — mint nodes from plain responsibility statements.
- `add_group` / `set_groups` / `delete_group` — organize nodes into folders.
- `add_links` / `update_links` / `delete_links` — typed relationships between nodes.
- `update_source_map` — link nodes and responsibilities to files and line ranges.

**Building & reconciliation:**
- `fill_container` — fill in a whole container's subtree at once when extracting from existing code (generation pipeline).
- `mark_implemented` — fold implemented work from the plan into the committed model.
- `flag_drift` / `reconcile_drift` — record undescribed behavior or stale claims, then advance the drift anchor.
- `update_nodes` / `delete_nodes` / `descope` / `move_nodes` / `move_responsibilities` — interactive edits and refinement (`descope` drops a node from the model while leaving its code in place).
- `set_model` / `set_node` — generation-pipeline primitives for whole-model / whole-subtree writes.
- `set_drift_watch` — pause drift detection while implementing.

## Drift detection & sync

Architecture models go stale as code changes. Scryer detects drift deterministically — no LLM — two ways: source-mapped nodes whose files changed since the last reconcile, and new files appearing in the project that the model doesn't cover yet.

When drift is detected:

1. A review surface and drift indicators flag the potentially drifted scopes — click through to the affected node pages.
2. Trigger a drift check to spawn your agent (Claude Code via `claude -p`, Codex via `codex exec`) with Scryer's MCP server attached. The agent reads the changed source files and updates the model only where code has actually diverged.
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
</content>
</invoke>
