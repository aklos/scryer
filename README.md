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
<video src="https://github.com/user-attachments/assets/4292467a-5f41-436e-b545-ad2785b64e73" width="100%" autoplay loop muted></video>
</p>

AI agents write code, but natural language is a lossy way to tell them what to build. Scryer gives you a shared visual model — you edit it in a drag-and-drop editor, the agent reads and modifies it through MCP. Once the model looks right, the agent generates code from it — `get_task` feeds work one unit at a time with dependency ordering, inherited contracts, and progress tracking.

Opinionated [C4](https://c4model.com/) hierarchy (system, container, component, operation/process/model), typed relationships, behavioral flows, contracts.

## Features

- **C4 Architecture Diagrams** — drag-and-drop editor for persons, systems, containers, components, operations, processes, and data models. Drill down through levels.
- **Behavioral Flows** — model user journeys, data pipelines, deploy sequences. Link flow steps to processes. Supports branching and decision points.
- **Data Models** — define typed properties on model nodes, visible on the canvas alongside your architecture.
- **Contracts** — expect/ask/never rules that constrain how AI agents implement your code. Inherited down the hierarchy. Expect items have pass/fail flags that gate the "ready" status.
- **Status Tracking** — three-status progression: proposed (planned), wip (code exists), ready (production-ready). "Ready" is gated — requires all contract expect items to pass. Agents must provide a reason for every status change.
- **Source Mapping** — link architecture nodes to files in your codebase with glob patterns and line ranges.
- **Groups** — organize containers into deployment or package groups for containers that ship together.
- **MCP Server** — AI agents connect to read, modify, and implement from your architecture model in real-time.
- **AI Advisor** — optional LLM-powered review that flags structural issues in your diagrams.
- **Implementation Workflow** — `get_task` feeds work to AI agents one unit at a time with dependency ordering, contract inheritance, and progress tracking. Build, mark wip, repeat.

## Getting started

Download the latest release for your platform from the [releases page](https://github.com/aklos/scryer/releases).

### Typical workflow

1. Tell your AI agent: *"Use scryer to model this project's architecture"*
2. The AI calls MCP tools — nodes appear in the visual editor in real-time
3. Review, drag things around, rename, remove, restructure
4. Tell the AI: *"Implement this model"*
5. The AI reads the model and generates code from it — marking each node as wip with a reason as it goes

## MCP server

The MCP server lets AI agents read and modify your architecture models. It ships bundled with the desktop app.

### Setup

In any project directory, run:

```bash
scryer-mcp init
```

This detects which AI tools you have installed (Claude Code, Codex) and writes project-scoped config for each:

- **Claude Code** — `.mcp.json`
- **Codex** — `.codex/config.toml`

Existing config files are preserved — only the `scryer` entry is added or updated. If neither tool is found in PATH, the command tells you.

### Manual setup

If you prefer to configure MCP manually, add scryer to your project config:

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

### What the MCP server provides

**Reading:**
- `get_model` — full model with all nodes, edges, flows, groups, source map
- `get_node` — scoped read of a subtree with internal/external edges and context
- `get_changes` — diff against baseline (what changed since you last looked)
- `get_task` — next implementation task with dependency ordering and inherited contracts
- `get_rules` — full C4 modeling rules and workflow guidance
- `get_structure` — annotated project directory tree (manifests, infrastructure, environments)

**Writing:**
- Add, update, and remove nodes and edges
- Define behavioral flows with branching (`set_flows`)
- Organize containers into groups (`set_groups`)
- Link nodes to source code (`update_source_map`)
- Validate the model against C4 rules (`validate_model`)

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
