# 98 Improvements for OpenDev (Inspired by OpenCode)

## Context

**Why:** OpenCode (TypeScript/Bun) and OpenDev (Rust) are both AI coding agents with similar scopes. OpenCode excels at fine-grained reactivity, rich provider support (22+), database-backed sessions, and a typed event bus. OpenDev excels at cancellation tokens, staged compaction, doom loop detection, and activity-based tool timeouts. This analysis identifies 100 concrete improvements for OpenDev by learning from OpenCode's strengths and addressing our own weaknesses.

**Methodology:** 3 exploration agents analyzed both codebases in parallel, followed by a Plan agent synthesizing findings into prioritized improvements.

**Priority:** P0 (critical) / P1 (high) / P2 (medium) / P3 (nice-to-have)
**Effort:** S (<1 day) / M (1-3 days) / L (3+ days)
**Source:** "OC" = learned from OpenCode, "INT" = internal analysis

---

## 1. TUI Performance & Rendering

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 1 | **Dirty-flag render skipping** — Add `dirty: bool` to `AppState`, set by `handle_event`, check before `terminal.draw()`. Currently every tick triggers full re-render even when nothing changed (`app.rs:355`). | P0 | S | OC |
| 2 | **Cached line builder for ConversationWidget** — `build_lines()` reconstructs all `Line` objects from all messages on every render. Cache output and only rebuild when messages change via generation counter. | P0 | M | OC |
| 3 | **Incremental message rendering** — Only rebuild lines for messages added since last render. Track `last_rendered_message_count` and append new lines instead of rebuilding from index 0. | P1 | M | OC |
| 4 | **Scroll acceleration curve** — OpenCode implements custom velocity-based scroll acceleration. OpenDev scrolls by fixed 3-line increments. Add momentum-based scrolling for held keys. | P2 | S | OC |
| 5 | **Viewport culling** — Only process messages within visible viewport + buffer zone, skip messages far above scroll offset entirely in `build_lines()`. | P1 | M | INT |
| 6 | **Theme system with multiple palettes** — OpenCode ships 35+ themes. OpenDev has hardcoded colors in `style_tokens.rs`. Add `Theme` struct with named palettes and `--theme` CLI flag. | P2 | L | OC |
| 7 | **Terminal background detection** — Detect light/dark via `COLORFGBG` env var or OSC 11 query. Currently all colors assume dark terminal. | P2 | S | OC |
| 8 | **Widget-level partial redraw** — Track which widgets changed and only redraw dirty regions instead of all 5 layout chunks every frame. | P1 | L | OC |
| 9 | **Reduce string allocations in markdown renderer** — Use `Cow<'a, str>` for spans that are substrings of original content instead of creating new `String` per span. | P2 | M | INT |
| 10 | **Double-buffered display messages** — Swap between two `Vec<DisplayMessage>` buffers so render reads from one while event handler writes to the other. | P3 | M | INT |

## 2. Provider & Streaming

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 11 | **Google Gemini provider adapter** — Add `GeminiAdapter` in `adapters/`. Gemini uses different message format (`parts` array). | P1 | M | OC |
| 12 | **Mistral provider adapter** — OpenAI-compatible but different tool calling format. | P2 | M | OC |
| 13 | **Groq provider adapter** — OpenAI-compatible with different rate limiting headers. | P2 | S | OC |
| 14 | **AWS Bedrock provider adapter** — Requires `aws-sigv4` crate for SigV4 request signing. | P2 | L | OC |
| 15 | **Azure OpenAI provider adapter** — Adds `api-version` query param and deployment-based URLs. | P2 | M | OC |
| 16 | **Ollama/local model provider** — OpenAI-compatible format, different base URL, no auth. | P2 | S | OC |
| 17 | **Eliminate payload cloning on retry** — `adapted_client.rs:47` clones payload unconditionally. For `None` adapter case, pass by reference. | P1 | S | INT |
| 18 | **Provider auto-detection from API key format** — Detect provider from key prefix (`sk-ant-` = Anthropic, `sk-` = OpenAI, `gsk_` = Groq). | P2 | S | OC |

## 3. Tool System

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 19 | **Tool execution timing & progress tracking** — Add `duration_ms` and `progress_pct` to `ToolResult::metadata`. Currently `ToolExecution` tracks `started_at` but result has no timing. | P1 | S | OC |
| 20 | **Granular tool state machine** — Replace boolean `finished` in `ToolExecution` with enum `ToolState { Pending, Running, Completed, Error, Cancelled }`. | P1 | S | OC |
| 21 | **Tool execution middleware pipeline** — Add `ToolMiddleware` trait with `before_execute`/`after_execute` hooks for cross-cutting concerns (timing, logging, permissions). | P2 | M | OC |
| 22 | **Tool output streaming callback** — Add optional `on_progress` callback to `BaseTool::execute` so tools like `bash` can stream output as it arrives. | P1 | M | OC |
| 23 | **Structured output via JSON Schema** — Support `response_format: { type: "json_schema" }` in LLM calls with retry on invalid JSON. | P1 | M | OC |
| 24 | **Tool parameter validation via JSON Schema** — Validate inputs with `jsonschema` crate before dispatching to `execute()` instead of manual validation. | P2 | M | OC |
| 25 | **Tool timeout configuration per tool** — Allow per-tool timeout config in `AppConfig` instead of hardcoded `IDLE_TIMEOUT`/`MAX_TIMEOUT` in bash. | P2 | S | INT |
| 26 | **Tool call deduplication** — When LLM requests identical tool calls in one response, deduplicate and return cached result. | P3 | S | INT |

## 4. Context & Compaction

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 27 | **Token-accurate counting via tiktoken** — Use `tiktoken-rs` crate instead of character-based heuristic for reliable compaction thresholds. | P1 | M | OC |
| 28 | **Sliding-window compaction** — For 500+ message sessions, keep recent N messages + compressed summary of earlier ones instead of staged approach only. | P2 | L | INT |
| 29 | **Compaction preview command** — `/compact-preview` shows what each stage would remove without performing it. | P3 | S | INT |
| 30 | **Per-message token count caching** — Cache token count when first computed (the `tokens` field is often `None`). Avoids re-counting on every compaction check. | P1 | S | INT |
| 31 | **Smart tool output summarization** — Before pruning at 85% stage, replace verbose outputs with 2-3 line summaries preserving key info instead of full removal. | P2 | M | OC |
| 32 | **Artifact index persistence** — Serialize `ArtifactIndex` to session file so it survives restarts. Currently in-memory only. | P1 | S | INT |

## 5. Session & History

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 33 | **Session forking** — Branch from a specific point in existing session. Copy messages to fork point, assign new ID. OpenCode has `session.fork()`. | P1 | M | OC |
| 34 | **Session reverting** — Integrate with `SnapshotManager` to revert session messages + workspace state to a specific step. Snapshots exist but no revert command exposed. | P1 | M | OC |
| 35 | **Session archiving** — Archive flag to hide old sessions from default listing without deletion. | P3 | S | OC |
| 36 | **SQLite session backend** — Replace JSON+JSONL with SQLite via `rusqlite` for atomic writes, indexing, and full-text search. | P1 | L | OC |
| 37 | **Session title auto-generation** — Generate short title after first assistant response. Currently `SessionMetadata.title` is always `None`. | P2 | S | OC |
| 38 | **Session sharing via public URL** — Upload anonymized transcript to configurable endpoint for shareable link. | P3 | L | OC |
| 39 | **Cross-session search** — `/search-sessions` command for content search across all sessions. With SQLite (#36) becomes simple FTS query. | P2 | M | OC |
| 40 | **Atomic session saves with WAL** — JSONL appends aren't atomic. Use SQLite WAL mode or implement write-ahead log. | P2 | M | INT |

## 6. Agent Loop

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 41 | **Multiple agent definitions with hot-switching** — Add `AgentDefinition` enum (build/plan/code/test) and `/agent <name>` runtime switching. Currently single main agent. | P1 | L | OC |
| 42 | **Agent handoff protocol** — Structured handoff message summarizing previous agent's work when switching, preserving continuity without full history. | P2 | M | OC |
| 43 | **Configurable thinking model per agent type** — Different thinking/critique models per agent definition. "Plan" agent might use stronger model, "code" agent faster one. | P2 | S | OC |
| 44 | **ReAct loop iteration metrics** — Track per-iteration LLM latency, token counts, tool times in structured format. Currently only `call_count` tracked. | P1 | S | INT |
| 45 | **Parallel tool execution with dependency graph** — Build dependency graph based on file paths so write tools targeting different files can run in parallel. | P2 | L | INT |
| 46 | **Agent abort with partial result preservation** — On Escape, preserve partial work (tool results, last assistant chunk) instead of discarding entirely. | P2 | M | INT |
| 47 | **Doom loop recovery strategies** — Beyond nudge messages: auto-compact context, switch model, or inject "step back" meta-prompt. | P2 | M | INT |

## 7. Config & Initialization

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 48 | **Config validation schema** — Validate loaded config against JSON Schema. Currently `ConfigLoader::merge` silently drops invalid fields. | P1 | M | OC |
| 49 | **Async config loading** — Replace `reqwest::blocking::Client` in `models_dev.rs:447` with async reqwest. Blocks tokio runtime at startup. | P1 | S | INT |
| 50 | **Lazy init for expensive subsystems** — LSP, MCP, embeddings should init on first use via `tokio::sync::OnceCell` instead of at startup. | P1 | M | OC |
| 51 | **Config hot-reload via file watcher** — Watch settings files and re-merge config at runtime without restart. | P3 | M | OC |
| 52 | **Environment-specific config profiles** — `dev`/`prod`/`fast` profiles via `OPENDEV_PROFILE` or `--profile` flag. | P3 | S | INT |
| 53 | **Config migration on schema changes** — `config_version` field with automatic migration logic for old config files. | P3 | M | INT |

## 8. Error Handling & Reliability

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 54 | **Systematic unwrap elimination** — 1621 `.unwrap()` calls. Audit critical paths (HTTP, session, agent loop). Start with `opendev-history` (132) and `opendev-web` (90). | P0 | L | INT |
| 55 | **Named error pattern with serializable errors** — Extend `StructuredError` to cover all crate error types with consistent codes and serializable objects. | P1 | M | OC |
| 56 | **Error recovery strategies per category** — Add `recovery_strategy() -> RecoveryAction` suggesting model fallback, context reduction, or user intervention. | P2 | M | INT |
| 57 | **Circuit breaker for provider APIs** — Open after N consecutive failures, cooldown period, probe request before closing. | P2 | M | INT |
| 58 | **Graceful MCP server failure degradation** — Remove failed server's tools from registry and notify user instead of propagating errors. | P1 | S | INT |
| 59 | **Panic handler with crash report** — Custom panic hook capturing backtrace + recent events + session state to `~/.opendev/crash/`. | P1 | S | INT |
| 60 | **Request ID tracing across LLM calls** — Unique request ID flowing through headers, logs, and error messages for end-to-end tracing. | P2 | S | INT |

## 9. Memory & Embeddings

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 61 | **Local embeddings via ONNX runtime** — Add local option using `ort` crate with `all-MiniLM-L6-v2`. Currently requires API calls for embeddings. | P2 | L | OC |
| 62 | **Embedding-based session search** — Semantic search across session messages using embedding cache. | P3 | M | OC |
| 63 | **Auto memory consolidation on session end** — Extract and store key learnings in playbook automatically. Currently requires manual `/memory-write`. | P2 | M | INT |
| 64 | **Embedding cache TTL and LRU eviction** — `EmbeddingCache` grows unboundedly. Add LRU eviction with configurable max size. | P2 | S | INT |
| 65 | **Reflection quality scoring** — Score reflections by number of supporting evidence points and recency. | P3 | M | INT |

## 10. Testing & Observability

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 66 | **Criterion benchmarks for hot paths** — Benchmark `build_lines()`, `MarkdownRenderer::render()`, `optimize_messages()`, `DoomLoopDetector::check()`. No benchmarks exist. | P1 | M | INT |
| 67 | **Performance regression CI gate** — `cargo bench` comparison against baseline, fail on >10% regression. | P2 | M | INT |
| 68 | **Structured tracing spans** — Replace ad-hoc `tracing::debug!` with structured spans capturing operation duration automatically. JSON formatted for parsing. | P2 | M | INT |
| 69 | **Integration test with mock LLM** — `wiremock`-based fake LLM API, send user message through full agent loop, verify tool execution flow. | P1 | M | INT |
| 70 | **TUI snapshot tests** — Use ratatui's `TestBackend` to capture rendered frames and compare against golden snapshots. | P1 | M | INT |
| 71 | **Tool execution fuzzing** — `cargo-fuzz` targets for bash dangerous command detection, file_edit fuzzy matching, patch hunk parsing. | P3 | M | INT |
| 72 | **Session persistence round-trip tests** — Verify `save_session` → `load_session` produces identical `Session` for edge cases (empty, Unicode, large). | P2 | S | INT |
| 73 | **Cost tracker accuracy tests** — Verify cost computation against known pricing including Anthropic >200K tier and cache discounts. | P2 | S | INT |

## 11. MCP & LSP Integration

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 74 | **OAuth flow for MCP servers** — Add OAuth 2.0 flow in `McpManager` for servers requiring token-based auth. | P2 | L | OC |
| 75 | **MCP server health monitoring** — Periodic heartbeat pings. Currently crashed servers only detected on tool call failure. | P1 | S | INT |
| 76 | **LSP diagnostics debouncing** — 100ms window to batch rapid diagnostic updates from language server. | P2 | S | OC |
| 77 | **MCP server auto-restart on crash** — Automatic restart with exponential backoff, configurable max retries. | P2 | M | INT |
| 78 | **LSP workspace symbol search tool** — Expose `workspace/symbol` as agent tool for cross-workspace symbol search. | P2 | M | INT |
| 79 | **MCP tool schema caching** — Cache `tools/list` response, refresh only on `tools/changed` notification. | P2 | S | INT |

## 12. Developer Experience

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 80 | **Skills from remote URLs** — URL-based skill loading (fetch markdown from HTTPS, cache locally). | P2 | M | OC |
| 81 | **Interactive model picker** — `/model` command with filterable list of available models from `models.dev` catalog. | P2 | M | OC |
| 82 | **Inline diff preview for file edits** — Render colored unified diff in TUI before applying. `diff_preview.rs` exists but isn't wired to TUI. | P1 | M | OC |
| 83 | **Session cost budget with auto-stop** — `--budget` flag for max USD per session. Pause agent when exhausted. | P2 | S | INT |
| 84 | **Command history with frecency** — Wire existing `managers/frecency.rs` to input history for up-arrow navigation. | P2 | S | INT |
| 85 | **Export session as markdown** — `/export` command rendering conversation as readable markdown. | P3 | S | INT |
| 86 | **Slash command argument autocompletion** — `AutocompleteEngine` supports `/` commands but not arguments. Add argument-aware completion. | P2 | M | OC |

## 13. Security & Permissions

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 87 | **Fine-grained permission RuleSet** — Ordered rules with glob/regex patterns and priority matching. Currently only basic `deny_patterns` per tool. | P1 | M | OC |
| 88 | **Per-directory permission scoping** — Different rules per directory (allow writes to `src/`, deny to `vendor/`). Currently global. | P2 | M | OC |
| 89 | **Compiled regex caching in approval rules** — `approval.rs:70` calls `Regex::new()` on every `matches()`. Compile and cache at creation time. | P1 | S | INT |
| 90 | **Secret detection in tool outputs** — Scan for API keys/tokens/passwords (`sk-`, `ghp_`, `Bearer`, base64) and redact before sending to LLM. | P1 | M | INT |
| 91 | **Fair reader-writer lock for session files** — Starvation-preventing RW lock with timeout. Current `flock` is basic. | P2 | M | OC |
| 92 | **Sandbox mode for untrusted operations** — Restrict bash to whitelist, prevent writes outside project directory. | P2 | L | INT |

## 14. State Management & Events

| # | Improvement | Pri | Eff | Src |
|---|------------|-----|-----|-----|
| 93 | **Typed event bus** — Replace stringly-typed events in `event_bus.rs` with typed enum variants for compile-time event matching. | P1 | M | OC |
| 94 | **Event filtering by subscriber interest** — Topic-based filtering so subscribers only receive relevant events. Currently broadcasts all to all. | P2 | S | OC |
| 95 | **Background task scheduler** — Instance-scoped scheduler for deferred work (embedding computation, auto-save, health checks). | P2 | M | OC |
| 96 | **File watcher with timeout protection** — `notify`-based watcher for working directory with timeout guard to kill stale watchers. | P2 | M | OC |
| 97 | **State snapshot for crash recovery** — Periodic serialization of essential `AppState` to temp file. On startup, detect incomplete sessions and offer resume. | P2 | M | INT |
| 98 | **Event replay for debugging** — Record all `AppEvent` variants to JSONL in debug mode. `--replay` flag feeds recorded events through event loop for deterministic reproduction. | P3 | L | INT |

---

## Summary by Priority

| Priority | Count | Key Items |
|----------|-------|-----------|
| **P0** | 3 | Dirty-flag rendering, cached lines, unwrap elimination |
| **P1** | 28 | Incremental rendering, viewport culling, provider adapters, tool progress, token counting, session forking/reverting, agent definitions, benchmarks, TUI snapshots, permissions, secret detection |
| **P2** | 48 | Themes, scroll acceleration, additional providers, middleware, compaction improvements, search, config profiles, circuit breaker, MCP/LSP improvements, DX features |
| **P3** | 19 | Double-buffering, session archiving/sharing, export, fuzzing, event replay |

## Critical Files

- `crates/opendev-tui/src/app.rs` — TUI state, render loop (dirty-flag, caching, state)
- `crates/opendev-tui/src/widgets/conversation.rs` — `build_lines()` hot path
- `crates/opendev-http/src/client.rs` — SSE streaming, retry improvements
- `crates/opendev-http/src/adapted_client.rs` — Provider adapters, payload cloning
- `crates/opendev-agents/src/react_loop.rs` — Agent loop metrics, doom loop recovery
- `crates/opendev-history/src/session_manager.rs` — Session persistence (SQLite migration)
- `crates/opendev-context/src/compaction.rs` — Token counting, artifact persistence
- `crates/opendev-runtime/src/event_bus.rs` — Typed events, filtering
- `crates/opendev-tools-core/src/registry.rs` — Tool middleware, state machine

## Verification

After implementing any improvement:
1. `cargo test --workspace` — all tests pass
2. `cargo clippy --workspace -- -D warnings` — zero warnings
3. `cargo build --release -p opendev-cli` — binary builds
4. `echo "hello" | opendev -p "hello"` — end-to-end smoke test
5. For TUI changes: launch `opendev` interactively and exercise the feature
