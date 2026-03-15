<div align="center">

  <img width="100px" src="public/logo.png" alt="Scryer logo" />

  <h1>scryer</h1>

  <p>
    <b>Visual architecture models that AI agents build from. Edit C4 diagrams in a drag-and-drop editor — AI agents read, modify, and implement the same model through MCP.</b>
    <br />
    <br />
    <a href="#features">Features</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#getting-started">Getting started</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#mcp-server">MCP server</a>
    <span>&nbsp;&nbsp;&bull;&nbsp;&nbsp;</span>
    <a href="#building-from-source">Building from source</a>
  </p>

</div>

<br/>

<p align="center">
<video src="https://github.com/user-attachments/assets/a67f5159-aac1-49b7-abba-dae11aad9499" width="100%" autoplay loop muted></video>
</p>

AI agents write code, but natural language is a lossy way to tell them what to build. Scryer gives you a shared visual model — you edit it in a drag-and-drop editor, the agent reads and modifies it through MCP. Once the model looks right, the agent generates code from it — `get_task` feeds work one unit at a time with dependency ordering, inherited contracts, and progress tracking.

Works with [Claude Code](https://docs.anthropic.com/en/docs/agents-and-tools/claude-code/overview) and [Codex](https://github.com/openai/codex) out of the box. Any agent that supports [MCP](https://modelcontextprotocol.io/) can read and write models. Agents that support [ACP](https://github.com/anthropics/agent-client-protocol) can also be spawned by Scryer for automated sync.

Opinionated [C4](https://c4model.com/) hierarchy (system, container, component, operation/process/model), typed relationships, behavioral flows, contracts.

## Features

- **C4 Architecture Diagrams** — drag-and-drop editor for persons, systems, containers, components, operations, processes, and data models. Drill down through levels. Code-level nodes (operations, processes, models) show in a compact list view.
- **Behavioral Flows** — model user journeys, data pipelines, deploy sequences. Supports branching and decision points. Flows serve as integration test specs — link them to test files via source mapping.
- **Contracts** — expect/ask/never rules that tell AI agents how to implement your code. Inherited down the hierarchy. Expect items have pass/fail flags that control when a node can be marked "ready".
- **Status Tracking** — three statuses: proposed (planned), wip (code exists), ready (verified). During implementation, agents mark nodes as wip. "Ready" is a separate verification step — the implementation must be complete (no stubs or TODOs), existing tests must pass, and all expect contract items must be satisfied.
- **Source Mapping** — link architecture nodes to files in your codebase with file patterns and line ranges. Click to open in your editor.
- **MCP Server** — AI agents connect to read, modify, and build from your architecture model in real-time.
- **Drift Detection & Sync** — Scryer tracks when source files change relative to the model. When drift is detected, click sync to have Scryer spawn the connected agent to update the model.
- **AI Advisor** — optional LLM-powered review that flags structural issues in your diagrams. Supports OpenAI, Anthropic, Google, Groq, Mistral, DeepSeek, and Ollama.
- **Implementation Workflow** — `get_task` gives AI agents one piece of work at a time, ordered by dependencies, with contracts inherited from parent nodes. Build, mark wip, repeat.
- **AI Tool Setup** — detects Claude Code and Codex, writes MCP config and auto-approve permissions for your project.

## Getting started

Download the latest release for your platform from the [releases page](https://github.com/aklos/scryer/releases).

### Typical workflow

1. Link your project directory in the app and enable AI tool integration when prompted (or run `scryer-mcp init`)
2. Tell your AI agent: *"Use scryer to model this project's architecture"*
3. The AI calls MCP tools — nodes appear in the visual editor in real-time
4. Review, drag things around, rename, remove, restructure
5. Tell the AI: *"Implement this model"*
6. The AI builds each piece one at a time, marking nodes as wip as it goes
7. When you're satisfied, ask the AI to verify — it checks for stubs, runs any existing tests, and confirms contract items pass before marking nodes as ready

As you work on code, Scryer detects when source files drift from the model. Click the sync button to have Scryer spawn your agent to update the model.

## Agent support

Scryer is built to work with **Claude Code** and **Codex** first.

- **MCP** (Model Context Protocol) — how agents read and write architecture models. Required for any agent integration.
- **CLI spawning** — how Scryer launches agents for automated sync. Claude Code is spawned via `claude -p` (uses your subscription), Codex via `codex exec` (uses your API key). Both get the Scryer MCP server attached automatically.
- **ACP** (Agent Client Protocol) — for agents that implement the full ACP handshake (e.g. via [claude-agent-acp](https://github.com/zed-industries/claude-agent-acp)). Scryer falls back to ACP if a `{name}-acp` binary is found on PATH.

When an agent connects via MCP, Scryer captures its identity from the protocol handshake. When sync is triggered, Scryer resolves that identity to a binary and launches it with the right flags. Claude Code and Codex are mapped automatically. For other agents, Scryer tries ACP conventions.

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

For Claude Code, you can also auto-approve Scryer's read tools so the agent doesn't prompt for every `get_model` call. The app can set this up for you, or add them manually to `.claude/settings.local.json`:

```json
{
  "permissions": {
    "allow": [
      "mcp__scryer__list_models",
      "mcp__scryer__get_model",
      "mcp__scryer__get_node",
      "mcp__scryer__get_rules",
      "mcp__scryer__get_changes",
      "mcp__scryer__get_structure"
    ]
  }
}
```

### What the MCP server provides

**Reading:**
- `get_model` — full model with all nodes, edges, flows, groups, source map. Name is optional — auto-resolves the model linked to the current working directory
- `get_node` — scoped read of a subtree with internal/external edges and context
- `get_changes` — diff against baseline (what changed since you last looked)
- `get_rules` — full C4 modeling rules and workflow guidance
- `get_structure` — annotated project directory tree (manifests, infrastructure, environments)

**Implementation:**
- `get_task` — next implementation task. When multiple containers are ready, presents a choice menu with groups. Scaffold tasks fire first for deployment groups. The model is the spec — agents must build exactly what it describes and clean up anything templates add that isn't in the model
- Add, update, and remove nodes and edges
- Define behavioral flows with branching (`set_flows`)
- Organize containers into groups (`set_groups`)
- Link nodes and flows to source code (`update_source_map`)
- Validate the model against C4 rules (`validate_model`)

## Drift detection & sync

Architecture models go stale as code changes. Scryer detects drift two ways: source-mapped nodes whose files changed since last sync, and new files appearing in the project that aren't covered by the model yet.

When drift is detected:

1. A **sync bar** appears at the bottom of the editor showing potentially drifted nodes — click any node name to navigate to it on the canvas
2. Click **Sync** to spawn your agent (Claude Code via `claude -p`, Codex via `codex exec`) with Scryer's MCP server attached. The canvas is locked during sync — cancel rolls back all agent changes
3. The agent receives a list of potentially drifted nodes, reads the changed source files, and updates the model only where code has actually diverged
4. Model changes appear in the editor in real-time. If nothing actually drifted, dismiss the notification to reset the baseline

For Claude Code, the MCP server config is passed inline via `--mcp-config`. For Codex, the project must have MCP already configured (via `scryer-mcp init` or the app's setup flow) since Codex reads MCP config from `.codex/config.toml`.

## Tech

Scryer is a [Tauri](https://tauri.app/) desktop app. The UI is written in [React](https://react.dev/) with [TypeScript](https://www.typescriptlang.org/) and the backend is written in [Rust](https://www.rust-lang.org/). Canvas rendering uses [ReactFlow](https://reactflow.dev/) with [d3-force](https://d3js.org/d3-force) for auto-layout.

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
