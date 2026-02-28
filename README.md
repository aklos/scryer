<div align="center">

  <img width="100px" src="public/logo.png" alt="Scryer logo" />

  <h1>scryer</h1>

  <p>
    <b>Visual planning tool for working with AI agents.</b>
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
<video src="https://github.com/user-attachments/assets/85862055-d280-4b42-823c-d8e0aa9ab8c5" width="100%" autoplay loop muted></video>
</p>

AI agents write code, but natural language is a lossy way to tell them what to build. Scryer gives you a shared visual model — you edit it in a drag-and-drop editor, the agent reads and modifies it through MCP. As software engineering moves up in abstraction, a visual model helps you reason about structure instead of staring at code. Opinionated [C4](https://c4model.com/) hierarchy (system, container, component, operation), typed relationships, behavioral flows, contracts.

## Features

- **C4 Architecture Diagrams** — drag-and-drop editor for persons, systems, containers, components, and operations. Drill down through levels.
- **Behavioral Flows** — model user journeys, data pipelines, deploy sequences. Link flow steps to components.
- **Data Models** — define typed properties on model nodes, visible on the canvas alongside your architecture.
- **Contracts** — always/ask/never rules that constrain how AI agents implement your code. Inherited down the hierarchy.
- **Source Mapping** — link architecture nodes to files in your codebase.
- **MCP Server** — AI agents connect to read, modify, and implement from your architecture model in real-time.
- **AI Advisor** — optional LLM-powered review that flags structural issues in your diagrams.
- **State Machine Workflow** — `get_task` feeds work to AI agents one unit at a time with dependency ordering, contract inheritance, and progress tracking. Build, mark done, repeat.

## Getting started

Download the latest release for your platform from the [releases page](https://github.com/aklos/scryer/releases).

### Typical workflow

1. Tell your AI agent: *"Use scryer to model this project's architecture"*
2. The AI calls MCP tools — nodes appear in the visual editor in real-time
3. Review, drag things around, rename, remove, restructure
4. Tell the AI: *"Implement this model"*
5. The AI reads the model and generates code from it

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

- Read the full model or scoped subtrees (`get_model`, `get_node`)
- Add, update, and remove nodes and edges
- Define behavioral flows (`set_flows`)
- Get implementation tasks in dependency order (`get_task`)
- Detect what changed since last read (`get_changes`)

## Tech

Scryer is a [Tauri](https://tauri.app/) desktop app. The UI is written in [React](https://react.dev/) with [TypeScript](https://www.typescriptlang.org/) and the backend is written in [Rust](https://www.rust-lang.org/). Canvas rendering uses [ReactFlow](https://reactflow.dev/) with [ELK](https://www.eclipse.org/elk/) for auto-layout.

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
