# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

## Build & Development Commands

```bash
# Build the entire workspace
cargo build --workspace

# Run all tests (skip doc-tests that have 0 tests)
cargo test --workspace --lib --tests

# Type/lint checks
cargo check --workspace
cargo clippy --workspace -- -D warnings

# Format code
cargo fmt --all

# Run a specific crate's tests
cargo test -p opendev-tui

# Run a single test by name
cargo test -p opendev-tui test_render_thinking_expanded

# Build and install release binary
cargo build --release -p opendev-cli
# Binary outputs to target/release/opendev (not opendev-cli)

# Auto-rebuild on file changes (requires cargo-watch)
cargo watch -x 'build --release -p opendev-cli'

# Web UI (React/Vite frontend)
cd web-ui && npm ci && npm run build
```

## Architecture Overview

OpenDev is a Rust workspace (edition 2024) with 21 crates under `crates/`. It is an open-source AI coding agent that spawns parallel agents, each bound to the LLM of your choice. The binary entry point is `opendev-cli`.

### Crate Map

```text
crates/
  opendev-cli         ← Binary entry point (clap CLI, dispatches to TUI/REPL/subcommands)
  opendev-tui         ← Terminal UI (ratatui + crossterm, async event loop)
  opendev-web         ← Web backend (axum + WebSocket, broadcasts agent events)
  opendev-repl        ← REPL loop, query enhancement (@file injection), message preparation
  opendev-agents      ← ReAct loop, thinking/critique phases, prompt composition
  opendev-runtime     ← Runtime services (approval, cost tracking, modes)
  opendev-config      ← Hierarchical config loading (project > user > env > defaults)
  opendev-models      ← Shared data types and models
  opendev-http        ← HTTP client, auth rotation, provider adapters (Anthropic, OpenAI, etc.)
  opendev-context     ← Context engineering (compaction stages, message validation)
  opendev-history     ← Session persistence (JSON per project, atomic writes)
  opendev-memory      ← Memory systems (embeddings, reflection, playbook)
  opendev-tools-core  ← Tool registry, BaseTool trait, dispatch
  opendev-tools-impl  ← 30+ tool implementations (bash, edit, file ops, web, agents)
  opendev-tools-lsp   ← LSP integration and language servers
  opendev-tools-symbol← AST-based symbol navigation
  opendev-mcp         ← Model Context Protocol integration
  opendev-channels    ← Channel routing
  opendev-hooks       ← Hook system
  opendev-plugins     ← Plugin manager
  opendev-docker      ← Docker runtime support
```
## Post-Change Workflow

**CRITICAL:** After every fix or feature, you MUST complete ALL of these steps before considering the task done. Do NOT skip any step.

### 1. Unit & Integration Tests

```bash
cargo test --workspace --lib --tests
```

All tests must pass. If you added new logic, add unit tests covering it. If the change touches cross-crate behavior, add or update integration tests in the relevant `tests/integration.rs`.

### 2. Lint & Type Checks

```bash
cargo clippy --workspace -- -D warnings
cargo check --workspace
```

Zero warnings, zero errors.

### 3. Rebuild Release Binary

```bash
cargo build --release -p opendev-cli
```

The binary at `target/release/opendev` is symlinked to `~/.local/bin/opendev`, so rebuilding automatically updates the installed version.

### 4. Real Simulation Test on TUI

**CRITICAL:** After rebuilding, you MUST run the actual binary against a real LLM to verify the feature works end-to-end. The `OPENAI_API_KEY` environment variable is already set. Run:

```bash
echo "hello" | opendev -p "hello"
```

Or for interactive TUI testing, launch `opendev` and exercise the feature manually. This catches issues that unit tests cannot — prompt composition, API payload format, TUI rendering, event flow, and real LLM response handling.

**Do NOT consider a feature complete until you have verified it works in the real TUI with a real LLM response.**

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Follow standard Rust naming conventions (snake_case functions, CamelCase types)
- **NEVER** add `Co-Authored-By` lines (e.g. `Co-Authored-By: Claude ...`) to commit messages

## Code Organization

- **Tests belong in separate files.** Do not put `#[cfg(test)] mod tests` inline in source files. Place unit tests in a sibling `tests.rs` or `tests/` directory and integration tests in `tests/`.
- **Keep source files focused.** When a module grows large, split it into submodules with a `mod.rs` or named modules. Each file should have a single clear responsibility.
- **New code follows existing structure.** Before adding a new file, check how the surrounding crate is organized and match that pattern.
