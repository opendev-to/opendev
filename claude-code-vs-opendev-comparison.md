# Claude Code vs OpenDev: Detailed Architectural Comparison

## Overview

Both are AI coding agents with ReAct loops, 40+ tools, multi-agent orchestration, and context management. Claude Code is Anthropic's proprietary TypeScript CLI (Bun/Node.js runtime, React/Ink TUI). OpenDev is an open-source Rust workspace with 22 crates (ratatui TUI, native binary).

---

## 1. Tool Interface & Registry

| Aspect | Claude Code (TypeScript) | OpenDev (Rust) |
|--------|-------------------------|----------------|
| **Core abstraction** | `Tool<Input, Output, P>` generic interface | `BaseTool` trait with `Send + Sync + Debug` |
| **Validation** | Zod v4 schemas (`inputSchema`, `outputSchema`) | JSON Schema with 4-phase manual validator |
| **Execution** | `call(args, context, canUseTool, parentMessage, onProgress?)` | `execute(args: HashMap<String, Value>, ctx: &ToolContext)` |
| **Result type** | Raw content blocks (text, image, tool_result) | `ToolResult { success, output, error, metadata, duration_ms, llm_suffix }` |
| **Registry** | Flat array from `getAllBaseTools()`, filtered by deny rules | `HashMap<String, Arc<dyn BaseTool>>` behind `RwLock` |
| **Registration** | Static -- all tools assembled at init | Dynamic -- `register()`/`unregister()` at runtime via interior mutability |
| **Tool count** | ~40+ (conditionally assembled) | ~40+ (registered at startup) |

### Claude Code Advantages

- **Richer tool metadata**: `isConcurrencySafe()`, `isReadOnly()`, `isDestructive()`, `interruptBehavior()`, `isSearchOrReadCommand()` -- each tool self-describes its safety properties
- **Dynamic descriptions**: `description(input, options)` generates context-aware descriptions based on actual input
- **Progress callbacks**: `onProgress` parameter enables streaming progress updates from tools to UI
- **Render methods**: Tools own their UI rendering (`renderToolUseMessage`, `renderToolResultMessage`, `renderGroupedToolUse`)
- **Auto-classifier input**: `toAutoClassifierInput()` generates compact representation for LLM-based safety classifier

### OpenDev Advantages

- **Interior mutability**: `RwLock<HashMap>` allows late registration (e.g., registering `SpawnSubagentTool` after `Arc<ToolRegistry>` is created)
- **Middleware pipeline**: `before_execute()` / `after_execute()` hooks for rate limiting, auditing, cost tracking -- Claude Code has no equivalent middleware layer
- **Alias system**: Backward-compatible name resolution (`register_alias("old_name", "new_name")`)
- **Fuzzy name resolution**: 5-step resolution chain (exact -> alias -> case-insensitive -> CamelCase->snake_case -> edit distance <=3 with suggestions)
- **Same-turn dedup cache**: Identical tool calls within a turn return cached result (skipped for Agent/subagent)
- **`llm_suffix`**: Hidden text appended to result for LLM guidance, invisible to user (e.g., "Error reading file. Try with correct path.")

---

## 2. Tool Execution Pipeline

### Claude Code Flow

```
Tool call -> validateInput() -> checkPermissions() -> auto-classifier (if auto mode)
-> hooks (PreToolUse) -> call() -> hooks (PostToolUse)
-> tool result budget check -> persist if > maxResultSizeChars -> return
```

### OpenDev Flow

```
Tool call -> resolve name (5 passes) -> normalize params (camelCase->snake_case, paths)
-> dedup cache check -> JSON Schema validate -> before_execute middleware
-> apply timeout -> execute() -> sanitize output (per-tool truncation + overflow storage)
-> after_execute middleware -> cache result -> return
```

| Step | Claude Code | OpenDev |
|------|-------------|---------|
| **Name resolution** | Exact match only (+ aliases via separate lookup) | 5-pass fuzzy: exact -> alias -> case-insensitive -> CamelCase->snake -> edit distance |
| **Param normalization** | None (Zod handles coercion) | Explicit: camelCase->snake_case (60+ mappings), path resolution, `~` expansion, whitespace trim |
| **Validation** | Zod v4 schema parse | 4-phase: required fields -> type constraints -> enum -> custom validators |
| **Permission check** | Per-tool `checkPermissions()` + global permission mode + auto-classifier | Middleware `before_execute()` -- generic, not tool-specific |
| **Hooks** | `PreToolUse`/`PostToolUse` shell hooks (user-configured) | `ToolMiddleware` trait (code-level, not shell) |
| **Output management** | `maxResultSizeChars` per tool -> persist to disk if exceeded | Per-tool truncation rules (Head/Tail/HeadTail) + overflow file storage (7-day retention) |
| **Deduplication** | None | Same-turn cache by `(tool_name, args_hash)` |

**Verdict:** Claude Code's permission system is more sophisticated (auto-classifier, tool-specific checks, shell hooks). OpenDev's execution pipeline is more robust (fuzzy resolution, normalization, dedup, sanitization strategies).

---

## 3. Specific Tool Implementations

### BashTool

| Feature | Claude Code | OpenDev |
|---------|-------------|---------|
| **Security** | `parseForSecurity()` AST analysis, sed interception | `is_dangerous()` blocklist (rm -rf /, sudo, mkfs, etc.) |
| **Background** | Explicit `run_in_background` + auto-background after 15s | Explicit `run_in_background` + auto-background for server commands |
| **Timeout** | Configurable, max from settings | Dual: idle timeout (60s no output) + absolute max (600s) |
| **Sandbox** | `SandboxManager` integration | None |
| **Output** | Raw stdout/stderr | stderr with `[stderr]` prefix, exit code in metadata |
| **Process cleanup** | Kill child process | Kill process group (`-pgid`) |

- **Claude Code wins**: AST-based security analysis is far more sophisticated than a blocklist. Sandbox support adds defense-in-depth.
- **OpenDev wins**: Dual timeout (idle + absolute) catches hanging processes that produce no output. Process group cleanup is more thorough.

### FileEditTool

| Feature | Claude Code | OpenDev |
|---------|-------------|---------|
| **Matching** | Strict mode (exact match required) | 9-pass fuzzy matching (tolerates whitespace/indentation) |
| **Concurrency** | No explicit locking | Per-file `Arc<Mutex>` locking (global LazyLock) |
| **Write strategy** | Direct write | Atomic write (UUID temp file -> rename) |
| **Formatting** | LSP diagnostic clearing | Auto-format detection and application |
| **Diff output** | Patch generation | Unified diff with 3-line context |
| **Max file size** | 1 GiB | Not explicitly capped (but Read caps at 10 MB) |

- **OpenDev wins**: Fuzzy matching, file locking, and atomic writes are more robust for concurrent agent scenarios.

### FileReadTool

| Feature | Claude Code | OpenDev |
|---------|-------------|---------|
| **Images** | Base64 return for multimodal | Binary detection and rejection |
| **PDFs** | Page extraction support | Not supported |
| **Notebooks** | Jupyter notebook rendering | Not supported |
| **Pagination** | `offset`/`limit` with line ranges | `offset`/`limit` with `next_offset` in metadata |
| **Size limit** | `maxResultSizeChars: Infinity` | Max 10 MB, 50 KB per read, 2000 chars/line |
| **Sensitive files** | Not mentioned | Warns for .env, .aws, .pem, .key |

- **Claude Code wins**: Multimodal support (images, PDFs, notebooks) is significantly more capable.
- **OpenDev wins**: Sensitive file warnings and per-line truncation prevent accidental secret exposure.

---

## 4. ReAct Loop

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Architecture** | `AsyncGenerator` yielding `StreamEvent\|Message` | `async fn run()` returning `AgentResult` |
| **Streaming** | Generator-based -- consumers pull events lazily | Callback-based -- `IterationEmitter` pushes events |
| **State** | Mutable `State` struct passed through loop | `LoopState` struct with fields for each concern |
| **Max iterations** | `maxTurns` (configurable) | `max_iterations` with wind-down summary call |
| **Wind-down** | Stops immediately | Calls LLM one final time WITHOUT tools for summary |

### Iteration Comparison

| Step | Claude Code | OpenDev |
|------|-------------|---------|
| 1 | Tool result budgeting | Safety checks (interrupts, limits) |
| 2 | Snip/micro/context-collapse compact | Context collection (todos, git, plan) |
| 3 | Autocompact check | Proactive reminders |
| 4 | Token budget blocking | LLM call (with streaming) |
| 5 | API call (streaming) | Response processing + metrics |
| 6 | Streaming tool execution (parallel!) | Turn decision (Continue/ToolCall/Complete) |
| 7 | Remaining tool execution | Doom loop detection |
| 8 | Memory prefetch + skill discovery | Tool execution (3-tier) |
| 9 | Attachment injection | Completion checks |
| 10 | Continue decision | Metrics + cleanup |

### Key Differences

**Claude Code advantage -- Streaming tool execution**: Claude Code can start executing tools *before the model finishes streaming* via `StreamingToolExecutor`. This overlaps LLM generation time with tool execution for lower latency.

**OpenDev advantage -- Doom loop detection**: OpenDev fingerprints every tool call (`name:args_hash`) in a 20-item sliding window and detects 1/2/3-step repeating cycles with 3-level escalation (Redirect -> Notify -> ForceStop). Claude Code has no equivalent.

**OpenDev advantage -- Wind-down summary**: When hitting max iterations, OpenDev calls the LLM one final time without tools to generate a graceful summary. Claude Code just stops.

**OpenDev advantage -- 3-tier tool execution**:
1. All `spawn_subagent`? -> Parallel (semaphore 25)
2. All read-only? -> Batched parallel (no semaphore)
3. Mixed -> Sequential with per-tool permissions

Claude Code executes tools with `isConcurrencySafe()` checks but doesn't have this explicit 3-tier architecture.

---

## 5. Context Compaction

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Approach** | Multiple independent systems behind feature flags | Single staged pipeline with 5 thresholds |
| **Microcompact** | Per-tool-result compaction (FileRead, Bash, Grep, etc.) | No equivalent (uses per-tool output truncation instead) |
| **Autocompact** | Threshold: `contextWindow - maxOutput - 13K` -> LLM summarization | 99% threshold -> LLM summarization |
| **Snip compact** | Feature-gated, removes middle sections | No equivalent |
| **Context collapse** | Feature-gated, progressive group collapse | 80% threshold -> progressive observation masking |
| **Reactive compact** | Triggered by actual `prompt_too_long` API error | No equivalent (proactive only) |
| **Token counting** | API-reported `input_tokens` + ~2.5 chars/token fallback | `cl100k_base`-style heuristic: `(words * 3 + 2) / 4` |
| **Sliding window** | Not mentioned | 500+ messages -> keep first + last 50, summarize middle |

### Claude Code's Compaction Pipeline

```
Microcompact (per-result) -> Snip (remove middle) -> Context Collapse (group summaries)
-> Autocompact (LLM summarization) -> Reactive Compact (last resort on API error)
```

### OpenDev's Staged Pipeline

```
70% -> Warning
80% -> Progressive observation masking (replace old tool results with refs)
85% -> Fast pruning (delete tool outputs <200 chars, protect recent 40K tokens)
90% -> Aggressive masking (mask 3 most recent vs 6 for 80%)
99% -> LLM-powered summarization with artifact index
500+ msgs -> Sliding window (first + last 50, compress middle)
```

**Verdict:** Claude Code has more granular compaction strategies that operate at different levels (per-result, per-section, per-conversation). OpenDev has a cleaner unified pipeline with explicit thresholds. Claude Code's reactive compaction (triggered by actual API errors) is a practical safety net OpenDev lacks. OpenDev's artifact index (tracking all files touched) that survives compaction is unique and valuable for maintaining continuity.

---

## 6. Message Cleaning

| Phase | Claude Code | OpenDev |
|-------|-------------|---------|
| **Internal filtering** | Strip system/local/progress messages | Remove `_msg_class: "internal"`, strip `_`-prefixed keys |
| **Whitespace removal** | Normalizes content blocks | Remove whitespace-only messages (preserve tool/tool_call-only) |
| **Role merging** | Not mentioned explicitly | Merge consecutive same-role user/assistant messages |
| **Orphan cleanup** | Strict tool_use <-> tool_result pairing | Remove tool messages without matching assistant tool_call_id |
| **Response cleaning** | Strip IDE context tags, signature blocks | Regex strip: chat template tokens, `<tool_call>`, `<function>`, system reminders |
| **Image handling** | Strip images before compaction | N/A (no multimodal) |

Claude Code cleans at the API boundary (`normalizeMessagesForAPI()`), focusing on strict message format compliance. OpenDev cleans in a 4-phase pipeline within `LlmCaller`, focusing on robustness (merging, orphan removal, artifact stripping). OpenDev's response cleaning is more aggressive -- stripping chat template tokens, echoed system reminders, and provider-specific artifacts that LLMs sometimes generate.

---

## 7. Subagent / Multi-Agent

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Spawning** | `AgentTool` -> `runAgent()` -> forked `query()` | `SpawnSubagentTool` -> `SubagentManager::spawn()` -> `ReactLoop` |
| **Context isolation** | `createSubagentContext()` -- separate file state cache, MCP connections | `ToolContext.is_subagent = true`, separate cancel token |
| **Tool restriction** | `ALL_AGENT_DISALLOWED_TOOLS` + `resolveAgentTools()` | `SubAgentSpec.tools` + `PermissionRule` per tool |
| **Background execution** | `run_in_background: true`, async agent | `background: true` in spec, spawns as background task |
| **Max iterations** | Not explicitly configurable per agent | 25 default (Standard), 100 for CodeExplorer (Simple runner) |
| **Nesting** | Agents can't spawn agents (unless ant-user) | Configurable via permission rules |
| **Agent definitions** | Files in `.claude/agents/`, frontmatter metadata | `SubAgentSpec` struct with 15+ fields |
| **Model override** | `"sonnet"`, `"opus"`, `"haiku"` aliases | Full model string or parent model inheritance |
| **Communication** | `<task-notification>` XML in user messages | Mailbox (file-based message passing) |
| **Worktree isolation** | `isolation: "worktree"` -- git worktree per agent | `IsolationMode::Worktree` -- same concept |
| **Runner variants** | Single `query()` loop for all | `StandardReactRunner` (full) vs `SimpleReactRunner` (stripped, 100 iter) |

### Claude Code Advantages

- **Streaming tool execution within agents**: Subagents benefit from the same streaming tool executor
- **Sidechain transcript**: Records full agent conversation for later resume via `SendMessage`
- **Prompt cache sharing**: Fork subagent shares parent's prompt cache prefix for cost efficiency
- **Agent-specific MCP servers**: Each agent definition can specify additional MCP servers

### OpenDev Advantages

- **Two runner types**: `SimpleReactRunner` (stripped-down, 100 iterations, for explorers) vs `StandardReactRunner` (full features, 25 iterations) -- right-sizes the loop per agent type
- **Mailbox system**: Persistent file-based message passing between agents with read/unread tracking, vs Claude Code's ephemeral `<task-notification>` injection
- **Reasoning effort capping**: Subagent reasoning_effort automatically capped (high -> medium) to save tokens
- **Per-tool permission patterns**: Glob-based patterns per tool per agent (not just allow/deny lists)

---

## 8. Team / Coordinator Mode

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Architecture** | Coordinator/Worker pattern via env var | TeamManager with leader + members |
| **Coordinator tools** | Only AgentTool, SendMessage, TaskStop | Leader has full tool access |
| **Worker tools** | Bash, Read, Edit, Grep, Glob, etc. | Configurable per team member |
| **Communication** | `<task-notification>` XML + `SendMessage` | Mailbox (file-based, persistent) |
| **Persistence** | Environment variable state | `~/.opendev/teams/{name}/team.json` |
| **Tmux integration** | Yes -- split panes, named windows | No |
| **Task tracking** | `TaskCreate/Get/Update/List` tools | `TeamMember.status` (Idle/Busy/Waiting/Done/Failed) |

Claude Code's coordinator is a strict orchestrator that can't touch files -- it only spawns workers and synthesizes results. This is a clean separation of concerns.

OpenDev's team manager is more flexible -- the leader retains full tool access and can both coordinate and execute. The file-based mailbox enables persistent cross-session communication. However, this flexibility means less architectural clarity about who does what.

---

## 9. Skills System

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Format** | Markdown with YAML frontmatter | Markdown with YAML frontmatter |
| **Discovery** | 6 sources: managed, user, project, plugin, bundled, MCP | 3 sources: project, user, built-in |
| **Frontmatter fields** | name, description, whenToUse, allowedTools, argumentHint, model, paths, hooks, version | name, description, namespace, source, model, agent |
| **Execution** | Forked sub-agent via `runAgent()` | Injected into context (not a separate agent) |
| **Conditional activation** | `paths` patterns for file-based activation | Not supported |
| **Embedded hooks** | Yes -- skills can define PreToolUse/PostToolUse hooks | No |
| **MCP skills** | Skills from MCP servers | No |
| **Token budgeting** | `estimateSkillFrontmatterTokens()` for context-aware loading | Not mentioned |

**Claude Code wins** with significantly richer skill system: more discovery sources, conditional activation by file path, embedded hooks, MCP integration, and execution as a forked sub-agent with its own tool restrictions.

---

## 10. Deferred Tools / ToolSearch

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Trigger** | Auto-enabled when tool count exceeds context threshold | Manually configured core vs. deferred |
| **Deferred signal** | `shouldDefer: true` on tool + `defer_loading: true` in API | `mark_as_core()` -- non-core tools are deferred |
| **Search algorithm** | Part-matching with scoring (exact=10-12, substring=5-6, searchHint=4, description=2) | Keyword scoring (name=3, description=1) + required prefix filter |
| **Direct selection** | `select:ToolName1,ToolName2` syntax | `select:Read,Edit,Grep` syntax |
| **Result format** | `tool_reference` blocks (API-native schema injection) | Full JSON Schema in text output + `activated_tools` metadata |
| **Never deferred** | AgentTool, BriefTool, SendUserFileTool, ToolSearchTool itself | Core tools (configurable at init) |

- **Claude Code advantage**: API-native `tool_reference` blocks mean the model sees properly structured schemas, not text approximations. Auto-enable based on context threshold is more ergonomic than manual configuration. Richer scoring algorithm with `searchHint` field.
- **OpenDev advantage**: Required prefix filter (`+web fetch` -> require "web" in name, search by "fetch") gives more precise results. `max_results` parameter controls result volume.

---

## 11. Project Instructions

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Primary file** | `CLAUDE.md` | `AGENTS.md`, `CLAUDE.md` |
| **Discovery path** | CWD -> home (bottom-up) | CWD -> git root (bottom-up) |
| **Local overrides** | `CLAUDE.local.md` (gitignored) | Not supported |
| **Rules directory** | `.claude/rules/*.md` (sorted) | Not supported |
| **Compatibility** | `.cursorrules`, `.github/copilot-instructions.md` | `.cursorrules`, `.github/copilot-instructions.md` |
| **Include directives** | `@path`, `@./relative`, `@~/home` | Not supported |
| **Size limits** | Not mentioned | 50 KB per file, 200 lines for MEMORY.md |
| **Remote instructions** | Not supported | `https://` URL support with 5s timeout |
| **Global** | `~/.claude/CLAUDE.md` | `~/.opendev/instructions.md`, `~/.opendev/AGENTS.md` |

**Claude Code wins**: `@include` directives, `.claude/rules/` directory for modular rules, and `CLAUDE.local.md` for gitignored personal overrides are practical features. OpenDev's remote URL support is unique but niche.

---

## 12. Session Persistence

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Format** | JSONL per session file | JSON (metadata) + JSONL (messages) split |
| **Location** | `~/.claude/projects/<slug>/<session-id>/` | `~/.opendev/projects/{id}/` or `~/.opendev/sessions/` |
| **Event sourcing** | No (append-only JSONL) | Yes -- `SessionEvent` enums with `EventEnvelope` (UUID, seq, timestamp) |
| **Tombstone support** | Yes (orphaned message removal) | Yes (`Tombstone { undo_to_seq, reason }`) |
| **Undo** | Not supported | Shadow git repo at `~/.opendev/snapshot/` -- per-step tree hash for perfect undo |
| **History** | `~/.claude/history.jsonl`, max 100 items | SessionIndex with search by title/date/channel |
| **Resume** | Full message chain restoration | Full restoration + mode reconciliation |
| **Fork** | Not supported | `SessionForked { source_session_id, fork_point }` event |

**OpenDev wins** significantly: Event sourcing with monotonic sequence numbers, shadow git repo for per-step undo, and session forking are powerful features for long-running agent sessions.

---

## 13. Retry & Error Handling

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Max retries** | 10 (default) | 3 (default) |
| **Backoff** | Exponential from 500ms base | Exponential from 2000ms base, factor 2.0 |
| **Jitter** | Not mentioned | +/-25% random factor |
| **Rate limit (429)** | Standard retry | Standard retry with Retry-After parsing |
| **Overload (529)** | Max 3 retries, foreground only | Treated as retryable |
| **Auth errors** | OAuth token refresh, credential cache clear | N/A (API key-based) |
| **Model fallback** | `FallbackTriggeredError` -> switch model | Not supported |
| **Persistent retry** | Unlimited for unattended mode (5min max backoff, 6hr cap) | Not supported |
| **Circuit breaker** | Not supported | Optional `CircuitBreaker` on `HttpClient` |
| **Cancellation** | `AbortController` propagation | `CancellationToken` with `tokio::select!` |

- **Claude Code wins**: Model fallback (Opus -> Sonnet on repeated failures), OAuth token refresh, and persistent retry mode for unattended sessions. More retries (10 vs 3) provides more resilience.
- **OpenDev wins**: Circuit breaker pattern (fail-fast after threshold), jitter for distributed load, and `Retry-After` header parsing for server-guided backoff.

---

## 14. Memory System

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **Directory memory** | `~/.claude/projects/<slug>/memory/` with `MEMORY.md` index | `~/.opendev/memory/` with embeddings-based search |
| **Memory types** | user, feedback, project, reference (4 types) | Embeddings-based semantic memory + reflection + playbook |
| **Session memory** | Auto-generated notes via forked subagent (periodic) | Not supported |
| **Dream system** | Orient -> Gather -> Consolidate -> Prune (background) | No equivalent |
| **Memory prefetch** | Async during model streaming | Not mentioned |
| **Team memory** | Shared memory paths (TEAMMEM feature) | Not mentioned |
| **Size limits** | 200 lines / 25 KB for MEMORY.md | Not mentioned |

- **Claude Code wins**: Session memory (auto-generated), dream system (background consolidation), and memory prefetch (async during streaming) are significantly more advanced. The 4-type taxonomy with structured frontmatter is well-designed.
- **OpenDev wins**: Embeddings-based semantic search for memory retrieval is more scalable than file-based memory for large knowledge bases.

---

## 15. Provider Abstraction

| Aspect | Claude Code | OpenDev |
|--------|-------------|---------|
| **SDK** | `@anthropic-ai/sdk` (official) | Custom `reqwest`-based HTTP client |
| **Providers** | Anthropic, Bedrock (AWS), Vertex (GCP) | Anthropic, OpenAI, Gemini, Azure, Groq, Mistral, Ollama, Bedrock, ChatCompletions |
| **Adapter trait** | No explicit trait (SDK handles it) | `ProviderAdapter` trait with 8 methods |
| **Auto-detection** | N/A (Anthropic-first) | API key prefix: `sk-ant-`->Anthropic, `sk-`->OpenAI, `gsk_`->Groq, `AIza`->Gemini |
| **Extended thinking** | Yes -- adaptive (4.6+) and enabled (3.7/4.0) modes | Yes -- via Anthropic adapter with budget tiers |
| **Prompt caching** | Yes -- `cache_control` blocks, beta header | Yes -- via Anthropic adapter |
| **Streaming** | Yes -- per-provider event parsing | Yes -- `parse_stream_event()` per adapter |

- **OpenDev wins decisively**: 9 providers vs 3. The `ProviderAdapter` trait with explicit `convert_request()`/`convert_response()` methods creates a clean, extensible abstraction. Auto-detection from API key prefix is user-friendly.
- **Claude Code wins**: Using the official Anthropic SDK gives access to beta features faster, and deep integration with Anthropic-specific features (structured outputs, advisor mode, task budgets).

---

## Overall Verdict by Category

| Category | Better Design | Reason |
|----------|--------------|--------|
| **Tool interface richness** | Claude Code | Self-describing safety properties, dynamic descriptions, render methods |
| **Tool execution robustness** | OpenDev | Fuzzy resolution, normalization, dedup, middleware, sanitization |
| **ReAct loop safety** | OpenDev | Doom loop detection, wind-down summary, 3-tier execution |
| **ReAct loop performance** | Claude Code | Streaming tool execution overlaps with LLM generation |
| **Context compaction** | Tie | CC has more strategies; OD has cleaner unified pipeline |
| **Message cleaning** | OpenDev | 4-phase pipeline with response artifact stripping |
| **Subagent architecture** | Tie | CC has cache sharing; OD has mailbox + dual runners |
| **Team coordination** | Claude Code | Clean coordinator/worker separation, tmux integration |
| **Skills system** | Claude Code | Richer metadata, more sources, conditional activation |
| **ToolSearch** | Claude Code | API-native schema injection, auto-enable |
| **Project instructions** | Claude Code | @include, rules directory, local overrides |
| **Session persistence** | OpenDev | Event sourcing, shadow git undo, session forking |
| **Retry/error handling** | Tie | CC has model fallback; OD has circuit breaker + jitter |
| **Memory system** | Claude Code | Session memory, dreams, prefetch |
| **Provider support** | OpenDev | 9 providers vs 3, clean adapter trait |

---

## Bottom Line

**Claude Code** has more advanced AI-native features (dreams, proactive agents, LLM-powered permissions, streaming tool execution) born from operating at scale with deep Anthropic API integration.

**OpenDev** has superior software engineering fundamentals (modularity via 22 Rust crates, type/memory safety, multi-provider support, event sourcing, doom loop detection, atomic writes, circuit breakers).

OpenDev is the better *foundation* to build on; Claude Code has more *innovation* in agent behavior.
