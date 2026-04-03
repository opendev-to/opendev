# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.8] - 2026-04-01

### Added

- Per-turn system prompt composition with section caching for efficient prompt reuse
- Semantic memory retrieval and message normalization for agents
- Plan edit review mode with plan recovery and improved error display

### Changed

- Align agent system with Claude Code patterns for tool dispatch and subagent handling
- Remove redundant ask-user subagent in favor of direct tool use
- Deduplicate token formatting and improve web-ui store architecture
- Remove external directory path restriction from tool dispatch
- Prevent unnecessary re-renders in web-ui MessageItem and DocumentationContent

### Fixed

- HTML comment headers in subagent templates causing 0 tool uses
- Subagent token tracking double-counting input tokens
- Windows tempfile paths with UNC prefix for CI compatibility
- Tests for API config and memory path resolution in CI

## [0.1.7] - 2026-03-31

### Added

- Read-only tool parallelization via consecutive batching for agents
- Human-readable memory system replacing unused Playbook
- Per-turn context attachment system for agents
- Team collaboration: TeamManager, file-based mailbox, team tools (create_team, send_message, delete_team)
- WorktreeManager for git worktree isolation
- Background agent support with `run_in_background` for subagents
- TaskManager for agent task lifecycle
- Sidechain transcripts for agent resume
- Task watcher detail view with enhanced footer and new keybindings
- Ctrl+B background hint after 2s of subagent execution
- `resume_with_messages()` for agent continuation

### Changed

- Redesigned system prompt and reminder architecture
- Renamed all tools to match Claude Code naming convention
- Removed restrictive prompt instructions that blocked conversational behavior

### Fixed

- Duplicate Ctrl+B hint in agent status line
- Stall detection, thinking finalization, and spinner improvements
- Actual line counts used for culled message placeholders
- Display-layer tool call boundary for background results

## [0.1.6] - 2026-03-30

### Added

- Microsandbox runtime detection, auto-start, and install script
- `opendev-sandbox` crate with microsandbox integration foundation
- Microsandbox runtime bundled in release archives and Homebrew formula
- Telegram remote session takeover support
- Per-session LLM debug logging enabled by default with truncated global log file
- Graceful exit message and session ID in TUI status bar
- Final summary nudge for Explorer subagent
- External directory approval prompt replacing hard path restrictions
- Startup/memory/size benchmarks comparing terminal coding agents
- crates.io badges, cargo install instructions, release badges in README

### Changed

- Context compaction now uses the model's actual context window instead of hardcoded 100k default
- Parallelized startup I/O to fix slow init on large codebases
- Background MCP connections overlapped with system prompt building
- Explorer max iterations lowered from 200 to 100 to prevent long stalls
- Session IDs now use hyphen-free hex format, shown on right side of status bar
- Refactored large files into modules for maintainability
- TUI markdown headings/bullets/bold reverted to neutral colors

### Fixed

- TOCTOU vulnerability in auth.json creation
- Interrupted tool calls no longer show misleading "Read N lines" in TUI
- Spinner freeze no longer blocks git snapshot operations
- Spinner race condition with orphaned subagent entries in TUI
- Double blank lines around headers collapsed in TUI rendering
- Web UI: scoped "New Session" disable guard to per-workspace
- `opendev-tools-symbol` dependency now uses workspace inheritance
- `floor_char_boundary` used for all string truncation to prevent panics on multi-byte chars
- Explorer nudge softened to reduce excessive tool calls

### Removed

- `opendev-docker` crate (replaced by `opendev-sandbox`)

## [0.1.2] - 2026-03-25

### Changed

- Provider docs now include a validated setup for custom OpenAI-compatible endpoints via `api_base_url`

### Fixed

- Environment variables now override stored `api_key` config values during provider auth resolution
- Homebrew release publishing now rewrites the generated formula class name before pushing to the tap
- Homebrew install docs now cover stale tap cleanup and local-dev symlink conflicts

## [0.1.1] - 2026-03-25

### Added

- Auto-version welcome panel from Cargo.toml (no more hardcoded version strings)
- Truncation notice in write_todos result to prevent LLM retry loops
- Todo panel auto-hide lifecycle with grace period
- Spinner and Ctrl+T hint in todo panel title
- Differentiated markdown heading styles with distinct colors and underlines

### Changed

- Default reasoning effort from high to medium
- Default autonomy level from Manual to Semi-Auto
- Improved todo panel styling: green header, arrow spinner, gold completed items
- Simplified todo panel to minimal 3-color scheme
- Improved incomplete_todos_nudge to guide workflow instead of being aggressive
- Removed parentheses from tool call displays, use space-separated format
- Extracted shared tool_line builders to eliminate 7 duplicated Span blocks

### Fixed

- Scroll-up showing blank space instead of conversation history
- Plan approval appearing stuck after user selection
- Plan panel box border off-by-one causing top line overflow
- Thinking trace headers rendered inline across interleaved blocks
- Orphan parents in task watcher by keeping finished subagents as covers
- Todo creation guard that made agent passive after creating todos
- Todo nudge guard: track task intent instead of last tool name

### Removed

- GitTool (replaced with 9 missing tool display entries and standardized tool display API)
- nextest config (plain cargo test is faster for this codebase)

## [0.1.0] - 2026-03-24

### Added

- Terminal UI (TUI) built with ratatui and crossterm
- Web UI (React/Vite) with WebSocket-based real-time agent monitoring
- 9 LLM provider support: OpenAI, Anthropic, Fireworks, Google, Groq, Mistral, DeepInfra, OpenRouter, Azure OpenAI
- Per-workflow model binding across 5 slots: Normal, Thinking, Compact, Critique, VLM
- Concurrent multi-agent sessions with independent model configurations
- 30+ built-in tools: bash, edit, file ops, web, agents, LSP, symbol navigation
- MCP (Model Context Protocol) integration for dynamic tool discovery
- Session persistence and history with JSON-based storage
- Hierarchical configuration system (project > user > env > defaults)
- Context engineering with multi-stage compaction
- Cross-platform support for macOS, Linux, and Windows
- CI pipeline with 3-platform test matrix (Ubuntu, macOS, Windows)
- Release automation with cargo-dist for 5 platform targets
- Shell installer (macOS/Linux), PowerShell installer (Windows), Homebrew tap

[0.1.8]: https://github.com/opendev-to/opendev/releases/tag/v0.1.8
[0.1.7]: https://github.com/opendev-to/opendev/releases/tag/v0.1.7
[0.1.6]: https://github.com/opendev-to/opendev/releases/tag/v0.1.6
[0.1.1]: https://github.com/opendev-to/opendev/releases/tag/v0.1.1
[0.1.2]: https://github.com/opendev-to/opendev/releases/tag/v0.1.2
[0.1.0]: https://github.com/opendev-to/opendev/releases/tag/v0.1.0
