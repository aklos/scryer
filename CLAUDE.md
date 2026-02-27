# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**scryer** is a visual architecture planning tool that integrates with AI coding assistants. Users and AI agents both interact with the same visual model — users through a drag-and-drop editor, AI agents through an MCP server. The shared surface lets users verify that the AI actually understands their architecture before implementation begins.

### Core Problem

AI coding assistants are bad at architecture. Text-based planning is lossy — you describe something, the AI rephrases it, you both think you agree, then the code comes out wrong. A visual representation forces precision that prose can't. scryer gives both the user and the AI a shared, visual planning surface where misalignment is caught before code gets written.

### Key Design Values

- Visual verification over chat-based planning.
- Encode proven patterns for AI agents: effective prompt templates, schema designs that guide AI behavior, and task decompositions that let cheaper models handle complex work reliably.
- Structured output schemas matter — small changes (e.g., adding a "passed" flag) can determine whether an agent returns garbage or correctly filters results.
- The spec format is the core deliverable. Everything else is interface to it.

## Tech Stack

- **Frontend**: React 19 + TypeScript + Vite 7
- **Desktop**: Tauri 2 (Rust backend)
- **Package manager**: pnpm
- **Dev environment**: Nix shell (`shell.nix`) provides all system dependencies

## Build & Dev Commands

```bash
pnpm install          # Install dependencies
pnpm tauri dev        # Run full app (builds sidecar + Tauri + Vite on :1420)
pnpm dev              # Run frontend only (Vite dev server on :1420)
pnpm tauri build      # Production build
pnpm build            # Frontend build only (tsc -b + vite build)
```

The sidecar (`scryer-mcp`) is built automatically by `pnpm tauri dev` and `pnpm tauri build` via the xtask crate.

No test runner or linter is configured yet.

## Architecture

**Local Tauri desktop app** that serves two interfaces:

- **Visual editor UI** — React frontend rendered in Tauri webview. Drag-and-drop C4 architecture diagrams.
- **MCP server** (`scryer-mcp`) — Claude Code, Codex, or other MCP-compatible tools connect to read and modify the same model state.

Both interfaces operate on the same underlying data. User drags nodes in the UI, AI agents call MCP tools like `set_model`, `add_nodes`, `get_model` — same model, two access points.

### Project Structure

- `src/` — React frontend (TypeScript)
- `src-tauri/` — Tauri/Rust backend
  - `src-tauri/src/lib.rs` — Tauri command handlers and app builder setup
  - `src-tauri/src/main.rs` — Binary entry point (calls into lib)
  - `src-tauri/capabilities/` — Tauri permission declarations
- `crates/scryer-core/` — Shared data types, model I/O, settings
- `crates/scryer-mcp/` — MCP server binary (stdio transport). Also provides `scryer-mcp init` for project setup.
- `crates/scryer-suggest/` — AI advisor (LLM-powered diagram review)
- `crates/xtask/` — Build tooling (sidecar compilation)
- `public/` — Static assets

### Frontend ↔ Backend Communication

The frontend calls Rust functions via Tauri's `invoke()` from `@tauri-apps/api/core`. Rust-side handlers are registered with `#[tauri::command]` in `src-tauri/src/lib.rs` and wired up in the Tauri builder's `.invoke_handler(tauri::generate_handler![...])`.

### Data Model

- Model state lives on disk as `.scry` JSON files in `~/.scryer/`
- No cloud dependency — everything runs locally
- C4 hierarchy: Person, System, Container, Component, Operation
- Nodes support contracts (always/ask/never rules), source mappings, and status tracking
- Behavioral flows model user journeys and link steps to architecture nodes

### MCP Setup for Development

The project includes `.mcp.json` (gitignored) for local dev. To set it up:

```bash
scryer-mcp init    # writes .mcp.json and .codex/config.toml in cwd
```

Or for development with hot recompilation, create `.mcp.json` manually:

```json
{
  "mcpServers": {
    "scryer": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "scryer-mcp"]
    }
  }
}
```

## License

Functional Source License 1.1, MIT Future License (FSL-1.1-MIT) — Copyright 2026 Alexander Klos
