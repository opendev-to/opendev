# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/opendev-to/opendev/releases/tag/v0.1.0
