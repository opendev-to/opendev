# Agent Framework Refactoring: Subagent Lifecycle, Teams, and TUI

## Context

OpenDev's subagent system is synchronous-only: parent blocks until subagent completes. No peer-to-peer communication, no persistent task state, no sidechain transcripts, no worktree isolation. This plan closes these gaps, inspired by Claude Code's architecture, across 6 crates with full TUI wiring and comprehensive testing.

---

## Phase 1: Task State Machine (`opendev-runtime`)

### New files
- `crates/opendev-runtime/src/task_manager/mod.rs`
- `crates/opendev-runtime/src/task_manager/types.rs`
- `crates/opendev-runtime/src/task_manager/tests.rs`

### Types (`types.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState { Pending, Running, Completed, Failed, Killed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub agent_type: String,
    pub description: String,           // 3-8 word label for display
    pub query: String,                 // full prompt
    pub session_id: String,
    pub state: TaskState,
    pub is_backgrounded: bool,
    pub was_async_spawn: bool,         // true = run_in_background from start
    pub created_at_ms: u64,            // millis since epoch
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: Option<u64>,
    pub tool_call_count: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub current_tool: Option<String>,
    pub result_summary: Option<String>,
    pub full_result: Option<String>,
    pub parent_task_id: Option<String>,
    pub team_id: Option<String>,
    pub activity_log: Vec<String>,     // rolling, max 200 entries
    pub recent_activities: Vec<ToolActivity>,  // last 5 (A2: rich typed activities)
    pub last_activity: Option<ToolActivity>,
    pub pending_messages: Vec<PendingMessage>,
    pub notified: bool,                // prevents duplicate completion notifications (B2)
    pub evict_after_ms: Option<u64>,   // grace period before cleanup (5s, corrected A3)
    pub retain: bool,                  // UI is viewing this task, block eviction (A6)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    pub from_agent: String,
    pub content: String,
    pub timestamp_ms: u64,
}
```

### TaskManager (`mod.rs`)

```rust
pub struct TaskManager {
    tasks: DashMap<String, TaskInfo>,
    interrupt_tokens: DashMap<String, InterruptToken>,
    max_concurrent: usize,  // default 5
    event_tx: Option<mpsc::UnboundedSender<TaskManagerEvent>>,
}

pub enum TaskManagerEvent {
    StateChanged { task_id: String, old: TaskState, new: TaskState },
    Progress { task_id: String, tool_name: String, tool_count: usize },
    MessageReceived { task_id: String, from: String },
}
```

**Methods**:
- `create_task(info: TaskInfo) -> String` — insert, publish StateChanged(Pending)
- `start_task(id)` — Pending→Running, set started_at
- `complete_task(id, success, summary, full_result)` — Running→Completed/Failed, set completed_at
- `fail_task(id, error)` — Running→Failed
- `kill_task(id)` — any→Killed, abort via interrupt_token
- `background_task(id)` — set is_backgrounded=true
- `update_progress(id, tool_name, tokens)` — increment counters, push activity
- `push_activity(id, line)` — append to activity_log (cap at 200)
- `queue_message(id, msg)` — append to pending_messages
- `drain_messages(id) -> Vec<PendingMessage>` — take and clear
- `get(id) -> Option<TaskInfo>`, `list() -> Vec<TaskInfo>`, `running_count() -> usize`
- `set_interrupt_token(id, token)`, `get_interrupt_token(id) -> Option<InterruptToken>`
- `mark_notified(id) -> bool` — atomically check+set notified flag, returns true if was unnotified (like Claude Code's idempotent pattern)
- `try_evict(id) -> bool` — evict if terminal + notified + past evict_after + !retain
- `set_retain(id, retain: bool)` — set/clear retain flag
- `set_evict_after(id, ms: u64)` — schedule eviction

**Edge case handling (learned from Claude Code)**:
- **Idempotent state transitions**: `kill_task` checks state != terminal first, returns Ok(()) if already killed
- **Atomic notified flag**: `mark_notified` uses `DashMap::get_mut` to check-and-set in one lock acquisition
- **Grace period**: Terminal tasks keep `evict_after_ms = now + 5_000` so TUI shows them briefly (5s, per A3)
- **Retain blocks eviction**: When TUI views a task detail, set `retain=true`; on exit, set `retain=false` + `evict_after=now`

### Migration from BackgroundAgentManager

`BackgroundAgentManager` in `crates/opendev-tui/src/managers/background_agents.rs` becomes a thin adapter:
- Internal `Arc<TaskManager>` replaces the `HashMap<String, BackgroundAgentTask>`
- `add_task()` → `task_manager.create_task()`
- `mark_completed()` → `task_manager.complete_task()`
- `get_task()` → `task_manager.get()`
- `tasks()` → `task_manager.list().into_iter().filter(|t| t.is_backgrounded)`
- Existing `BackgroundAgentState` enum maps directly to `TaskState`

### Files to modify
- `crates/opendev-runtime/src/lib.rs` — add `pub mod task_manager;` + re-exports
- `crates/opendev-tui/src/managers/background_agents.rs` — delegate to TaskManager
- No AppState struct changes needed (BackgroundAgentManager wraps internally)

---

## Phase 2: Sidechain Transcripts (`opendev-history`)

### New files
- `crates/opendev-history/src/sidechain/mod.rs`
- `crates/opendev-history/src/sidechain/types.rs`
- `crates/opendev-history/src/sidechain/writer.rs`
- `crates/opendev-history/src/sidechain/reader.rs`
- `crates/opendev-history/src/sidechain/tests.rs`

### Storage: `~/.opendev/sessions/{parent_session_id}/agents/{agent_id}.jsonl`

### Types

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub seq: u64,
    pub ts: u64,  // millis since epoch
    #[serde(flatten)]
    pub entry: EntryKind,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "k")]
pub enum EntryKind {
    #[serde(rename = "sys")]  SystemPrompt { content: String },
    #[serde(rename = "ast")]  AssistantMsg { content: String, tool_calls: Option<Vec<Value>> },
    #[serde(rename = "tr")]   ToolResult { call_id: String, name: String, output: String, ok: bool },
    #[serde(rename = "tok")]  Tokens { inp: u64, out: u64 },
    #[serde(rename = "st")]   StateChange { from: String, to: String },
}
```

### SidechainWriter

```rust
pub struct SidechainWriter {
    file: BufWriter<File>,
    seq: u64,
    path: PathBuf,
    bytes_written: u64,
}
```

- `new(session_dir, agent_id) -> io::Result<Self>` — create dirs, open append-mode
- `append(entry: EntryKind) -> io::Result<()>` — serialize JSON line, flush, increment seq + bytes
- **Max size 50MB**: When `bytes_written > 50_000_000`, log warning but continue (don't rotate — append-only simplicity)
- **Partial write safety**: Each append is a single `write_all` + newline + flush. Reader skips malformed lines.

### SidechainReader

- `open(session_dir, agent_id) -> io::Result<Self>`
- `entries() -> impl Iterator<Item = Result<TranscriptEntry, _>>` — line-by-line, skip parse errors (learned from Claude Code: `parseJSONL` skips invalid lines)
- `tail(n) -> Vec<TranscriptEntry>` — read all, take last N (for TUI preview)
- `into_messages() -> Vec<Value>` — reconstruct LLM-compatible message array for resume
- **Concurrent read safety**: Reader opens file independently. Writer only appends. JSONL line boundaries guarantee reader sees complete lines or truncated last line (which is skipped).

### Integration
- Add `sidechain_writer: Option<SidechainWriter>` to `RunnerContext`
- `StandardReactRunner` writes entries after each LLM response and tool result
- `SimpleReactRunner` writes entries similarly
- **Fire-and-forget** pattern: if write fails, log warning, continue execution (don't block agent)

### Files to modify
- `crates/opendev-history/src/lib.rs` — add module
- `crates/opendev-agents/src/subagents/runner/mod.rs` — add field
- `crates/opendev-agents/src/subagents/runner/standard.rs` — write entries
- `crates/opendev-agents/src/subagents/runner/simple.rs` — write entries
- `crates/opendev-agents/src/subagents/manager/spawn.rs` — create writer

---

## Phase 3: Background Agent Execution Mode

### 3A: `run_in_background` Parameter

**Modify**: `crates/opendev-tools-impl/src/agents/spawn.rs`

Add fields to `SpawnSubagentTool`:
```rust
task_manager: Option<Arc<TaskManager>>,
```

Add to `parameter_schema()`:
```json
"run_in_background": {
    "type": "boolean",
    "description": "Run agent in background. Returns task_id immediately; you'll be notified on completion.",
    "default": false
}
```

**In `execute()`, when `run_in_background == true`:**

```rust
let run_in_background = args.get("run_in_background")
    .and_then(|v| v.as_bool()).unwrap_or(false);

// Also check spec.background (auto-background agents)
let run_in_background = run_in_background || self.manager.get(agent_type)
    .map(|s| s.background).unwrap_or(false);

if run_in_background {
    let task_id = uuid::Uuid::new_v4().to_string()[..12].to_string();
    let cancel_token = CancellationToken::new();
    let interrupt_token = InterruptToken::new();

    // Register task
    if let Some(ref tm) = self.task_manager {
        tm.create_task(TaskInfo {
            task_id: task_id.clone(),
            agent_type: agent_type.to_string(),
            description: description.unwrap_or(agent_type).to_string(),
            query: task.to_string(),
            session_id: child_session_id.clone(),
            state: TaskState::Pending,
            is_backgrounded: true,
            was_async_spawn: true,
            ..Default::default()
        });
        tm.set_interrupt_token(&task_id, interrupt_token.clone());
    }

    // Notify TUI
    if let Some(ref tx) = self.event_tx {
        let _ = tx.send(SubagentEvent::BackgroundSpawned {
            task_id: task_id.clone(),
            query: task.to_string(),
            session_id: child_session_id.clone(),
            interrupt_token: interrupt_token.clone(),
        });
    }

    // Spawn detached task
    let manager = Arc::clone(&self.manager);
    let registry = Arc::clone(&self.tool_registry);
    let http = Arc::clone(&self.http_client);
    let event_tx = self.event_tx.clone();
    let task_manager = self.task_manager.clone();
    let parent_model = self.parent_model.clone();
    // ... clone all needed data

    tokio::spawn(async move {
        if let Some(ref tm) = task_manager { tm.start_task(&task_id); }

        // Create BackgroundEventCallback (existing pattern from tui_runner)
        let bg_callback = BackgroundProgressCallback::new(event_tx.clone(), task_id.clone());

        match manager.spawn(
            &agent_type, &task, &parent_model,
            registry, http, &wd,
            Arc::new(bg_callback),
            None, None,
            parent_max_tokens, reasoning_effort,
            Some(cancel_token), debug_logger.as_deref(),
        ).await {
            Ok(result) => {
                let success = result.agent_result.success && !result.agent_result.interrupted;
                let summary = truncate(&result.agent_result.content, 200);
                let full = result.agent_result.content.clone();
                if let Some(ref tm) = task_manager {
                    tm.complete_task(&task_id, success, &summary, &full);
                }
                if let Some(ref tx) = event_tx {
                    let _ = tx.send(SubagentEvent::BackgroundCompleted {
                        task_id: task_id.clone(), success,
                        result_summary: summary, full_result: full,
                        cost_usd: 0.0, tool_call_count: result.tool_call_count,
                    });
                }
            }
            Err(e) => {
                if let Some(ref tm) = task_manager {
                    tm.fail_task(&task_id, &e.to_string());
                }
                if let Some(ref tx) = event_tx {
                    let _ = tx.send(SubagentEvent::BackgroundCompleted {
                        task_id: task_id.clone(), success: false,
                        result_summary: e.to_string(), full_result: String::new(),
                        cost_usd: 0.0, tool_call_count: 0,
                    });
                }
            }
        }
    });

    // Return immediately
    return ToolResult::ok(format!(
        "Background agent started.\ntask_id: {task_id}\nAgent: {agent_type}\n\n\
         Running in background. You'll be notified when it completes. Continue your work."
    ));
}
```

**New SubagentEvent variants** in `crates/opendev-tools-impl/src/agents/events.rs`:
```rust
BackgroundSpawned { task_id, query, session_id, interrupt_token },
BackgroundCompleted { task_id, success, result_summary, full_result, cost_usd, tool_call_count },
BackgroundProgress { task_id, tool_name, tool_count },
BackgroundActivity { task_id, line },
```

**New `BackgroundProgressCallback`** in `crates/opendev-tools-impl/src/agents/events.rs`:
Implements `SubagentProgressCallback`, emits `BackgroundProgress` + `BackgroundActivity` events. Modeled after `BackgroundEventCallback` in `tui_runner/mod.rs:121-172`.

### 3B: Mid-Execution Backgrounding Enhancement

**Current flow** (already works):
1. Ctrl+B → `interrupt_token.request_background()` → react loop yields `AgentResult::backgrounded()`
2. `handle_agent_backgrounded()` in TUI cleans up display

**Enhancement** — re-spawn in background after Ctrl+B:

In `crates/opendev-cli/src/tui_runner/mod.rs`, inside the main agent listener task, after receiving `AgentResult::backgrounded()`:

```rust
if result.backgrounded && !result.messages.is_empty() {
    let task_id = format!("{:012x}", rand::random::<u64>());
    let messages = result.messages.clone();
    let query_summary = /* extract from first user message */;

    // Write sidechain
    if let Ok(mut writer) = SidechainWriter::new(&session_dir, &task_id) {
        for msg in &messages {
            let _ = writer.append(msg_to_entry(msg));  // fire-and-forget
        }
    }

    // Create fresh InterruptToken for the background task
    let bg_interrupt = InterruptToken::new();

    // Register in TaskManager
    if let Some(ref tm) = task_manager {
        tm.create_task(TaskInfo {
            task_id: task_id.clone(),
            is_backgrounded: true,
            was_async_spawn: false,  // Ctrl+B'd
            state: TaskState::Running,
            ..
        });
        tm.set_interrupt_token(&task_id, bg_interrupt.clone());
    }

    // Notify TUI
    let _ = event_tx.send(AppEvent::SetBackgroundAgentToken {
        task_id: task_id.clone(), query: query_summary,
        session_id: child_session_id.clone(), interrupt_token: bg_interrupt.clone(),
    });

    // Spawn background continuation
    let bg_callback = BackgroundEventCallback { tx: event_tx.clone(), task_id: task_id.clone(), tool_count: Arc::new(AtomicUsize::new(0)) };
    tokio::spawn(async move {
        let resume_result = runtime.resume_with_messages(
            messages, &system_prompt, Some(&bg_callback),
            Some(&bg_interrupt), false,
        ).await;

        match resume_result {
            Ok(r) => {
                if let Some(ref tm) = task_manager {
                    tm.complete_task(&task_id, r.success, &summary, &full);
                }
                let _ = event_tx.send(AppEvent::BackgroundAgentCompleted { task_id, success, .. });
            }
            Err(e) => {
                if let Some(ref tm) = task_manager {
                    tm.fail_task(&task_id, &e.to_string());
                }
                let _ = event_tx.send(AppEvent::BackgroundAgentCompleted { task_id, success: false, .. });
            }
        }
    });
}
```

**Note**: `runtime.resume_with_messages()` is a new method on `AgentRuntime` that continues from existing message history instead of starting fresh. It reuses the same agent config but creates a new react loop iteration from the saved messages.

### 3C: Background Result Injection (Wiring)

**Existing mechanism works**: `pending_queue` in AppState already handles this.

Flow:
1. `BackgroundAgentCompleted` event → `handle_background_agent_completed()`
2. Handler calls `task_manager.mark_notified(&task_id)` — returns false if already notified (prevents duplicate)
3. If mark_notified returns true: push `PendingItem::BackgroundResult` to `pending_queue`
4. If `!agent_active`: call `drain_next_pending()` immediately
5. `drain_next_pending()` sends `\x00__BG_RESULT__{json}` sentinel via `user_message_tx`
6. TUI runner detects sentinel, calls `runtime.inject_background_result()`
7. Agent processes result, finishes turn, `handle_agent_finished()` calls `drain_next_pending()` again

---

## Phase 4: Enhanced SubAgentSpec

Add to `crates/opendev-agents/src/subagents/spec/types.rs` (all `#[serde(default)]`):

```rust
pub permission_mode: Option<AgentPermissionMode>,  // Inherit, Autonomous, Manual
pub isolation: Option<IsolationMode>,               // None, Worktree
pub background: bool,                               // auto-background on spawn
pub omit_instructions: bool,                        // skip CLAUDE.md/AGENTS.md
```

### Files to modify
- `crates/opendev-agents/src/subagents/spec/types.rs` — add fields + enums
- `crates/opendev-agents/src/subagents/custom_loader/parser.rs` — parse new keys
- `crates/opendev-agents/src/subagents/manager/spawn.rs` — honor new fields

---

## Phase 5: Agent Team System

### 5A: Mailbox (`crates/opendev-runtime/src/mailbox/mod.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub id: String,
    pub from: String,
    pub content: String,
    pub timestamp_ms: u64,
    pub read: bool,
    pub msg_type: MessageType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType { Text, ShutdownRequest, ShutdownResponse { approved: bool }, Idle }

pub struct Mailbox { inbox_path: PathBuf }
```

**Concurrency**: Use `fd-lock` for file locking (already a dependency). Each `send()` acquires exclusive lock, reads JSON array, appends, writes back, releases. Retry 10x with 5-100ms backoff (matches Claude Code pattern).

**Edge cases**:
- **Corrupt inbox JSON**: If deserialization fails, rename to `.corrupt.{timestamp}`, create fresh empty array. Log warning.
- **Missing inbox file**: `send()` creates it. `receive()` returns empty vec.
- **Unbounded growth**: `send()` checks array length; if >1000 messages, trim oldest read messages.

### 5B: TeamManager (`crates/opendev-runtime/src/team_manager/mod.rs`)

```rust
pub struct TeamConfig {
    pub name: String,
    pub leader: String,
    pub leader_session_id: String,
    pub members: Vec<TeamMember>,
    pub created_at_ms: u64,
}

pub struct TeamMember {
    pub name: String,
    pub agent_type: String,
    pub task_id: String,
    pub status: TeamMemberStatus,
    pub joined_at_ms: u64,
}

pub enum TeamMemberStatus { Idle, Busy, Waiting, Done, Failed }

pub struct TeamManager {
    teams_dir: PathBuf,
    active_teams: DashMap<String, TeamConfig>,
    task_manager: Arc<TaskManager>,
}
```

**Orphaned team cleanup**: On `TeamManager::new()`, scan `teams_dir` for existing team configs. If `leader_session_id` doesn't match any active session, mark as orphaned. Cleanup on next `create_team()` or explicit `cleanup()`.

### 5C: Team Tools (`crates/opendev-tools-impl/src/agents/team_tools.rs`)

#### CreateTeamTool
- Parameters: `team_name: String`, `members: [{ name: String, agent_type: String, task: String }]`
- Creates team via TeamManager
- For each member: spawns background task (reuses SpawnSubagentTool logic with `run_in_background=true`)
- Each member gets enhanced system prompt with team context:
  ```
  You are part of team '{name}'. Your role: {member.name}.
  Teammates: [list].
  Use send_message to communicate with teammates.
  The team leader can send you messages and shutdown requests.
  Check for messages between tool calls.
  ```
- Returns: `"Team '{name}' created with {N} members.\nMembers: {list with task_ids}"`

#### SendMessageTool
- Parameters: `to: String`, `message: String`, `summary: String` (5-10 words)
- `to == "*"`: broadcast to all team members except sender
- Writes to recipient's `Mailbox::send()`
- Returns confirmation with delivery status

#### DeleteTeamTool
- Parameters: `team_name: String`
- Sends `ShutdownRequest` to all members via mailbox
- Waits 3s for graceful exit (check task states)
- Kills remaining via `task_manager.kill_task()`
- Cleanup team files

### 5D: Teammate Mailbox Polling

In `crates/opendev-agents/src/react_loop/phases/llm_call.rs`, add before LLM call:

```rust
// Drain mailbox messages before each LLM call
if let Some(ref mailbox) = ctx.mailbox {
    match mailbox.receive() {
        Ok(msgs) if !msgs.is_empty() => {
            for msg in msgs {
                let content = match msg.msg_type {
                    MessageType::ShutdownRequest => format!(
                        "[SHUTDOWN REQUEST from team leader]: {}\n\
                         Wrap up your current work and call task_complete.",
                        msg.content
                    ),
                    MessageType::Text => format!(
                        "[Message from teammate '{}']: {}",
                        msg.from, msg.content
                    ),
                    _ => continue,
                };
                messages.push(json!({ "role": "user", "content": content }));
            }
        }
        _ => {} // No messages or error — continue silently
    }
}
```

**Polling frequency**: Every react loop iteration (~1-5s between LLM calls). No explicit sleep needed.

### Files to create
- `crates/opendev-runtime/src/mailbox/mod.rs`
- `crates/opendev-runtime/src/mailbox/tests.rs`
- `crates/opendev-runtime/src/team_manager/mod.rs`
- `crates/opendev-runtime/src/team_manager/tests.rs`
- `crates/opendev-tools-impl/src/agents/team_tools.rs`
- `crates/opendev-tools-impl/src/agents/team_tools_tests.rs`

### Files to modify
- `crates/opendev-runtime/src/lib.rs` — add modules
- `crates/opendev-agents/src/subagents/runner/mod.rs` — add `mailbox: Option<Arc<Mailbox>>` to RunnerContext
- `crates/opendev-agents/src/react_loop/phases/llm_call.rs` — drain mailbox

---

## Phase 6: Git Worktree Isolation

### New file: `crates/opendev-runtime/src/worktree/mod.rs`

```rust
pub struct WorktreeManager { base_dir: PathBuf }

pub struct WorktreeInfo { pub path: PathBuf, pub branch: String, pub agent_id: String }
pub enum MergeResult { Clean, Conflict { files: Vec<String> }, NoChanges }
```

- `create(repo_root, agent_id)` — `git worktree add -b opendev/agent-{short_id} {path}`
- `cleanup(agent_id, has_changes)` — remove if no changes, retain if changes exist
- `merge_back(agent_id, target)` — merge agent branch into target

**Edge cases (from Claude Code)**:
- `create()` validates slug before side effects (fail fast)
- Symlink failures for node_modules etc are non-fatal (log warning, continue)
- If worktree path already exists, generate unique suffix
- On agent crash: worktree remains on disk for manual inspection/resume

---

## Phase 7: TUI Wiring (The Critical Phase)

This phase wires everything from backend through events to display. Every detail matters.

### 7A: New AppEvent Variants

Add to `crates/opendev-tui/src/event/mod.rs`:

```rust
// Background spawn (async from start) — mapped from SubagentEvent::BackgroundSpawned
BackgroundAgentSpawned {
    task_id: String,
    query: String,
    session_id: String,
    interrupt_token: InterruptToken,
},

// Team events
TeamCreated { team_id: String, leader_name: String, member_names: Vec<String> },
TeamMemberStatusChanged { team_id: String, member_name: String, status: String },
TeamMessageSent { from: String, to: String, content_preview: String },
TeamDeleted { team_id: String },
```

### 7B: SubagentEvent → AppEvent Bridge

In `crates/opendev-cli/src/tui_runner/mod.rs`, extend the subagent event bridge (lines 278-352) to handle new variants:

```rust
SubagentEvent::BackgroundSpawned { task_id, query, session_id, interrupt_token } => {
    // Map to BOTH events (register token + spawn notification)
    let _ = sa_tx.send(AppEvent::SetBackgroundAgentToken {
        task_id: task_id.clone(), query: query.clone(),
        session_id, interrupt_token,
    });
    AppEvent::BackgroundAgentSpawned { task_id, query, session_id: String::new(), interrupt_token: InterruptToken::new() }
    // Note: interrupt_token already sent via SetBackgroundAgentToken, this is for TUI toast
}
SubagentEvent::BackgroundCompleted { task_id, success, result_summary, full_result, cost_usd, tool_call_count } => {
    AppEvent::BackgroundAgentCompleted { task_id, success, result_summary, full_result, cost_usd, tool_call_count }
}
SubagentEvent::BackgroundProgress { task_id, tool_name, tool_count } => {
    AppEvent::BackgroundAgentProgress { task_id, tool_name, tool_count }
}
SubagentEvent::BackgroundActivity { task_id, line } => {
    AppEvent::BackgroundAgentActivity { task_id, line }
}
```

### 7C: Event Dispatch Routing

In `crates/opendev-tui/src/app/event_dispatch.rs`, add to `handle_event()`:

```rust
AppEvent::BackgroundAgentSpawned { task_id, query, .. } => {
    self.handle_background_agent_spawned(&task_id, &query);
}
AppEvent::TeamCreated { team_id, leader_name, member_names } => {
    self.handle_team_created(&team_id, &leader_name, &member_names);
}
AppEvent::TeamMemberStatusChanged { team_id, member_name, status } => {
    self.handle_team_member_status(&team_id, &member_name, &status);
}
AppEvent::TeamMessageSent { from, to, content_preview } => {
    self.handle_team_message(&from, &to, &content_preview);
}
AppEvent::TeamDeleted { team_id } => {
    self.handle_team_deleted(&team_id);
}
```

### 7D: New Handler Files

#### `crates/opendev-tui/src/app/handle_background.rs` — Extend existing

Add `handle_background_agent_spawned()`:
```rust
pub(super) fn handle_background_agent_spawned(&mut self, task_id: &str, query: &str) {
    // Toast notification
    let desc: String = query.chars().take(40).collect();
    self.state.toasts.push(Toast::new(
        format!("Background agent started: {desc}"),
        ToastLevel::Info,
    ));
    // bg_agent_manager already handled by SetBackgroundAgentToken
    self.state.dirty = true;
}
```

Modify `handle_background_agent_completed()` to use `task_manager.mark_notified()`:
```rust
pub(super) fn handle_background_agent_completed(&mut self, ...) {
    // Mark notified atomically (prevent duplicate injection)
    let should_inject = self.state.bg_agent_manager.mark_notified(&task_id);

    // Toast
    let level = if success { ToastLevel::Success } else { ToastLevel::Error };
    let msg = if success {
        format!("Agent done: {} ({} tools)", desc, tool_call_count)
    } else {
        format!("Agent failed: {}", &result_summary[..50.min(result_summary.len())])
    };
    self.state.toasts.push(Toast::new(msg, level).with_duration(Duration::from_secs(5)));

    // Queue result for injection (only if not already notified)
    if should_inject && success {
        self.state.pending_queue.push_back(PendingItem::BackgroundResult { ... });
        if !self.state.agent_active {
            self.drain_next_pending();
        }
    }
    self.state.dirty = true;
}
```

#### `crates/opendev-tui/src/app/handle_team.rs` — New file

```rust
pub(super) fn handle_team_created(&mut self, team_id: &str, leader: &str, members: &[String]) {
    self.state.active_team = Some(TeamDisplayState {
        team_id: team_id.to_string(),
        leader_name: leader.to_string(),
        members: members.iter().map(|n| TeamMemberDisplay::new(n)).collect(),
        created_at: Instant::now(),
    });
    self.state.toasts.push(Toast::new(
        format!("Team '{}' created with {} members", team_id, members.len()),
        ToastLevel::Info,
    ));
    self.state.dirty = true;
}

pub(super) fn handle_team_member_status(&mut self, team_id: &str, member: &str, status: &str) {
    if let Some(ref mut team) = self.state.active_team {
        if let Some(m) = team.members.iter_mut().find(|m| m.name == member) {
            m.status = status.parse().unwrap_or(TeamMemberStatus::Idle);
        }
    }
    self.state.dirty = true;
}

pub(super) fn handle_team_message(&mut self, from: &str, to: &str, preview: &str) {
    if let Some(ref mut team) = self.state.active_team {
        // Add to both sender and receiver message logs
        let entry = TeamMessageEntry {
            from: from.to_string(), to: to.to_string(),
            content: preview.to_string(), timestamp: Instant::now(),
        };
        for m in &mut team.members {
            if m.name == from || m.name == to {
                m.message_log.push(entry.clone());
                m.last_message_at = Some(Instant::now());
            }
        }
    }
    // Toast only if task watcher is closed
    if !self.state.task_watcher_open {
        self.state.toasts.push(Toast::new(
            format!("{from} -> {to}: {}", &preview[..30.min(preview.len())]),
            ToastLevel::Info,
        ).with_duration(Duration::from_secs(2)));
    }
    self.state.dirty = true;
}

pub(super) fn handle_team_deleted(&mut self, _team_id: &str) {
    self.state.active_team = None;
    self.state.toasts.push(Toast::new("Team disbanded".into(), ToastLevel::Info));
    self.state.dirty = true;
}
```

### 7E: AppState Additions

In `crates/opendev-tui/src/app/state.rs`:

```rust
pub active_team: Option<TeamDisplayState>,
pub task_watcher_detail: Option<usize>,     // expanded cell index
pub task_watcher_messages: Option<usize>,   // message log view index
```

### 7F: SubagentDisplayState Extensions

In `crates/opendev-tui/src/widgets/nested_tool/state.rs`:

```rust
pub cost_usd: f64,
pub run_in_background: bool,
pub background_hint_shown: bool,
pub foreground_start: Option<Instant>,  // set when subagent starts in foreground
```

### 7G: Ctrl+B Hint System

**In `crates/opendev-tui/src/app/tick.rs`**, add to `handle_tick()`:

```rust
// Ctrl+B hint: show after 2s of foreground subagent execution
for subagent in &mut self.state.active_subagents {
    if !subagent.finished && !subagent.backgrounded
        && !subagent.background_hint_shown && !subagent.run_in_background
    {
        if let Some(start) = subagent.foreground_start {
            if start.elapsed() > Duration::from_secs(2) {
                subagent.background_hint_shown = true;
                self.state.dirty = true;
            }
        }
    }
}
```

**In `crates/opendev-tui/src/widgets/conversation/spinner.rs`**, when rendering a spawn_subagent tool:

```rust
// After spinner line, if matched subagent has hint
if subagent.background_hint_shown && !subagent.backgrounded {
    spans.push(Span::styled(
        "  Ctrl+B to background",
        Style::default().fg(theme.dim_grey).add_modifier(Modifier::ITALIC),
    ));
}
```

**Dismissal**: Hint disappears when `subagent.finished` or `subagent.backgrounded` becomes true.

### 7H: Task Watcher Panel Enhancements

**File**: `crates/opendev-tui/src/widgets/background_tasks.rs`

#### Cell type icons
In `render_cell()`, vary the title icon by source:
- Async spawn (`was_async_spawn`): `"\u{2699}"` (gear)
- Ctrl+B'd: `"\u{21b3}"` (arrow)
- Team member: `"\u{25cf}"` (filled circle) — check `task.team_id.is_some()`
- Team leader: star `"\u{2606}"` + gold border

#### Enhanced footer
Extend the footer line computation:
```rust
let footer = format!(
    "{status} {elapsed} {tool_count} tools{tokens}{cost}{msgs}",
    tokens = if token_count > 0 { format!(" \u{00b7} {}tok", format_compact(token_count)) } else { String::new() },
    cost = if cost_usd > 0.001 { format!(" \u{00b7} ${:.3}", cost_usd) } else { String::new() },
    msgs = if msg_count > 0 { format!(" \u{00b7} {}msgs", msg_count) } else { String::new() },
);
```

#### Detail view (Enter key)
When `state.task_watcher_detail == Some(idx)`:
- Render full-width single-cell overlay instead of grid
- Content: full activity log (scrollable with j/k), tokens, cost, last 10 tool calls
- For team members: include recent message log
- Press Esc or Enter to return to grid

#### Message log view (m key, team members only)
When `state.task_watcher_messages == Some(idx)`:
- Scrollable chronological message list
- Each message: `[HH:MM:SS] from -> to: content`
- Direction icons: `\u{25b6}` sent, `\u{25c0}` received
- Press Esc to return

### 7I: Keybinding Changes

In `crates/opendev-tui/src/app/key_handler.rs`, within task watcher key handling:

```rust
KeyCode::Enter => {
    if self.state.task_watcher_detail.is_some() {
        self.state.task_watcher_detail = None;  // close detail
    } else if self.state.task_watcher_messages.is_some() {
        self.state.task_watcher_messages = None;  // close messages
    } else {
        self.state.task_watcher_detail = Some(self.state.task_watcher_focus);
    }
}
KeyCode::Char('m') => {
    // Only for team member cells
    if self.state.active_team.is_some() {
        self.state.task_watcher_messages = Some(self.state.task_watcher_focus);
    }
}
KeyCode::Char('r') => {
    // Restart failed/killed task
    self.try_restart_task(self.state.task_watcher_focus);
}
KeyCode::Char('t') => {
    // Toggle sort: time vs status
    self.state.task_watcher_sort_by_status = !self.state.task_watcher_sort_by_status;
}
KeyCode::Esc => {
    if self.state.task_watcher_detail.is_some() {
        self.state.task_watcher_detail = None;
    } else if self.state.task_watcher_messages.is_some() {
        self.state.task_watcher_messages = None;
    } else {
        self.state.task_watcher_open = false;
    }
}
```

Update help footer:
```rust
" q:close  hjkl:nav  J/K:scroll  Enter:detail  x:kill  m:msgs  r:retry "
```

### 7J: Status Bar

In `crates/opendev-tui/src/widgets/status_bar.rs`, extend background task display:

```rust
// Team status (when active)
if let Some(ref team) = self.team_status {
    let (busy, total) = team;
    spans.push(separator());
    spans.push(Span::styled(
        format!("Team:{busy}/{total}"),
        Style::default().fg(theme.cyan).bold(),
    ));
}
```

### 7K: Toast Patterns

| Event | Level | Duration | Message |
|-------|-------|----------|---------|
| Background spawn | Info | 3s | `"Background agent started: {desc}"` |
| Background completed | Success | 5s | `"Agent done: {desc} ({N} tools, {elapsed})"` |
| Background failed | Error | 5s | `"Agent failed: {desc}: {first 50 chars}"` |
| Ctrl+B | Info | 3s | `"Sent to background (Ctrl+P to view)"` |
| Team created | Info | 3s | `"Team '{name}' created with {N} members"` |
| Team member done | Success | 3s | `"[{member}] completed task"` |
| Team member failed | Error | 5s | `"[{member}] failed: {first 30 chars}"` |
| Agent message | Info | 2s | `"{from} -> {to}: {first 30 chars}"` (suppressed when watcher open) |

### 7L: Render Pipeline Changes

In `crates/opendev-tui/src/app/render.rs`, update overlay rendering:

```rust
// Task watcher overlay (with detail/message sub-views)
if self.state.task_watcher_open {
    if let Some(idx) = self.state.task_watcher_detail {
        self.render_task_detail(frame, area, idx);
    } else if let Some(idx) = self.state.task_watcher_messages {
        self.render_task_messages(frame, area, idx);
    } else {
        self.render_task_watcher(frame, area);
    }
}
```

Pass team_status to StatusBarWidget:
```rust
StatusBarWidget::new()
    .team_status(self.state.active_team.as_ref().map(|t| {
        let busy = t.members.iter().filter(|m| m.status == TeamMemberStatus::Busy).count();
        (busy, t.members.len())
    }))
```

---

## Phase 8: Comprehensive Testing Strategy

### 8A: Unit Tests (per module)

#### TaskManager tests (`crates/opendev-runtime/src/task_manager/tests.rs`)
```rust
#[test] fn test_create_task_and_retrieve()
#[test] fn test_lifecycle_pending_running_completed()
#[test] fn test_lifecycle_pending_running_failed()
#[test] fn test_lifecycle_running_killed()
#[test] fn test_kill_already_completed_is_noop()  // idempotent
#[test] fn test_kill_already_killed_is_noop()      // idempotent
#[test] fn test_complete_already_completed_is_noop()
#[test] fn test_mark_notified_returns_true_once()  // atomic check-and-set
#[test] fn test_mark_notified_returns_false_on_second_call()
#[test] fn test_evict_requires_terminal_and_notified()
#[test] fn test_evict_blocked_by_retain()
#[test] fn test_evict_blocked_before_grace_period()
#[test] fn test_evict_succeeds_after_grace_period()
#[test] fn test_running_count()
#[test] fn test_max_concurrent_enforcement()
#[test] fn test_concurrent_access()  // spawn 10 threads, all manipulate tasks
#[test] fn test_activity_log_capped_at_200()
#[test] fn test_drain_messages_clears_queue()
#[test] fn test_set_retain_blocks_eviction()
```

#### Sidechain tests (`crates/opendev-history/src/sidechain/tests.rs`)
```rust
#[test] fn test_write_and_read_entries()
#[test] fn test_tail_returns_last_n()
#[test] fn test_into_messages_reconstructs_history()
#[test] fn test_corrupt_line_skipped()   // write valid, inject garbage line, read still works
#[test] fn test_empty_file_reads_empty()
#[test] fn test_concurrent_write_and_read()  // writer appends while reader iterates
#[test] fn test_large_file_performance()     // 10K entries, verify read time <1s
#[test] fn test_writer_creates_directories()
#[test] fn test_reader_nonexistent_file_error()
```

#### Mailbox tests (`crates/opendev-runtime/src/mailbox/tests.rs`)
```rust
#[test] fn test_send_and_receive()
#[test] fn test_receive_marks_read()
#[test] fn test_peek_does_not_mark_read()
#[test] fn test_empty_mailbox_returns_empty()
#[test] fn test_send_creates_file_if_missing()
#[test] fn test_concurrent_writes()         // 5 threads write simultaneously
#[test] fn test_corrupt_inbox_recovery()    // write garbage JSON, verify recovery
#[test] fn test_message_cap_trims_old_read()  // >1000 messages trimmed
#[tokio::test] async fn test_poll_with_timeout()
#[tokio::test] async fn test_poll_returns_on_new_message()
```

#### TeamManager tests (`crates/opendev-runtime/src/team_manager/tests.rs`)
```rust
#[test] fn test_create_and_get_team()
#[test] fn test_delete_team_cleans_files()
#[test] fn test_list_teams()
#[test] fn test_update_member_status()
#[test] fn test_duplicate_team_name_error()
```

#### WorktreeManager tests (`crates/opendev-runtime/src/worktree/tests.rs`)
```rust
#[test] fn test_create_worktree()         // needs git repo fixture
#[test] fn test_cleanup_no_changes()      // verify worktree removed
#[test] fn test_cleanup_with_changes()    // verify worktree retained
#[test] fn test_create_fails_gracefully_on_invalid_repo()
```

### 8B: Integration Tests

#### Background agent lifecycle (`crates/opendev-tools-impl/tests/background_agent.rs`)
```rust
#[tokio::test]
async fn test_background_spawn_returns_task_id() {
    // Setup: mock SubagentManager that returns after 100ms
    // Call execute() with run_in_background=true
    // Assert: ToolResult::ok contains "task_id:"
    // Assert: TaskManager has task in Pending/Running state
    // Wait 200ms, assert: TaskManager has task in Completed state
}

#[tokio::test]
async fn test_background_spawn_sends_events() {
    // Setup: capture SubagentEvent via channel
    // Spawn background agent
    // Assert: receives BackgroundSpawned event
    // Wait for completion
    // Assert: receives BackgroundCompleted event
}

#[tokio::test]
async fn test_background_kill_stops_agent() {
    // Setup: mock SubagentManager that runs for 10s
    // Spawn background
    // Kill via TaskManager after 100ms
    // Assert: task state = Killed within 500ms
}

#[tokio::test]
async fn test_duplicate_notification_prevention() {
    // Complete a task
    // Call mark_notified twice
    // Assert: first returns true, second returns false
}
```

#### Team lifecycle (`crates/opendev-tools-impl/tests/team_lifecycle.rs`)
```rust
#[tokio::test]
async fn test_create_team_spawns_members() {
    // Create team with 2 members
    // Assert: 2 tasks created in TaskManager
    // Assert: team config written to disk
}

#[tokio::test]
async fn test_send_message_delivers_to_mailbox() {
    // Create team, send message from leader to member
    // Assert: member's mailbox contains message
}

#[tokio::test]
async fn test_delete_team_kills_members() {
    // Create team, delete it
    // Assert: all member tasks killed
    // Assert: team files cleaned up
}

#[tokio::test]
async fn test_shutdown_propagation() {
    // Create team, send shutdown request
    // Assert: members receive shutdown in mailbox
}
```

#### Sidechain resume (`crates/opendev-history/tests/sidechain_resume.rs`)
```rust
#[test]
fn test_write_resume_cycle() {
    // Write 10 transcript entries
    // Read back via into_messages()
    // Assert: message count and content match
    // Write 5 more entries
    // Read back again, assert 15 total
}
```

### 8C: TUI Unit Tests

#### Event dispatch tests (`crates/opendev-tui/src/app/tests.rs`)
```rust
#[test]
fn test_background_spawned_event_adds_toast() {
    let mut app = App::new();
    app.handle_event(AppEvent::BackgroundAgentSpawned { task_id: "t1".into(), .. });
    assert_eq!(app.state.toasts.len(), 1);
    assert!(app.state.toasts[0].message.contains("Background agent started"));
}

#[test]
fn test_background_completed_queues_result() {
    let mut app = App::new();
    // Setup: add a background task first
    app.handle_event(AppEvent::SetBackgroundAgentToken { task_id: "t1".into(), .. });
    app.handle_event(AppEvent::BackgroundAgentCompleted { task_id: "t1".into(), success: true, .. });
    assert_eq!(app.state.pending_queue.len(), 1);
}

#[test]
fn test_background_completed_duplicate_not_queued() {
    // Complete same task twice → only one item in pending_queue
}

#[test]
fn test_ctrl_b_hint_shown_after_2s() {
    let mut app = App::new();
    app.state.active_subagents.push(SubagentDisplayState {
        foreground_start: Some(Instant::now() - Duration::from_secs(3)),
        background_hint_shown: false,
        ..
    });
    app.handle_tick();
    assert!(app.state.active_subagents[0].background_hint_shown);
}

#[test]
fn test_ctrl_b_hint_not_shown_before_2s() {
    let mut app = App::new();
    app.state.active_subagents.push(SubagentDisplayState {
        foreground_start: Some(Instant::now() - Duration::from_millis(500)),
        ..
    });
    app.handle_tick();
    assert!(!app.state.active_subagents[0].background_hint_shown);
}

#[test]
fn test_team_created_sets_active_team() {
    let mut app = App::new();
    app.handle_event(AppEvent::TeamCreated { team_id: "t1".into(), leader_name: "lead".into(), member_names: vec!["a".into(), "b".into()] });
    assert!(app.state.active_team.is_some());
    assert_eq!(app.state.active_team.as_ref().unwrap().members.len(), 2);
}

#[test]
fn test_team_message_adds_to_log() {
    // Create team, send message, verify message_log
}

#[test]
fn test_team_message_toast_suppressed_when_watcher_open() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    app.handle_event(AppEvent::TeamMessageSent { .. });
    assert_eq!(app.state.toasts.len(), 0);  // suppressed
}

#[test]
fn test_task_watcher_enter_opens_detail() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    app.state.task_watcher_focus = 0;
    // Simulate Enter key
    app.handle_key_task_watcher(KeyCode::Enter);
    assert_eq!(app.state.task_watcher_detail, Some(0));
}

#[test]
fn test_task_watcher_esc_closes_detail_first() {
    let mut app = App::new();
    app.state.task_watcher_open = true;
    app.state.task_watcher_detail = Some(0);
    app.handle_key_task_watcher(KeyCode::Esc);
    assert_eq!(app.state.task_watcher_detail, None);
    assert!(app.state.task_watcher_open);  // watcher still open
}

#[test]
fn test_render_before_drain_includes_new_events() {
    assert!(App::should_render_before_draining(&AppEvent::BackgroundAgentSpawned { .. }));
}
```

### 8D: Real Simulation Tests

After `cargo build --release -p opendev-cli`:

```bash
# Test 1: Background agent spawn
echo "Spawn a background Explore agent to find all struct definitions in the agents crate" | opendev -p "test bg"
# Verify: toast appears, task watcher shows running task, completes with result

# Test 2: Multiple background agents
echo "Spawn 3 background agents in parallel: one to explore agents crate, one to explore tools crate, one to explore runtime crate" | opendev -p "test parallel bg"
# Verify: 3 tasks in watcher, all complete, results injected

# Test 3: Ctrl+B hint
echo "Explore the entire codebase and explain the architecture in detail" | opendev -p "test hint"
# Verify: after 2s, "Ctrl+B to background" hint appears
# Press Ctrl+B: agent moves to background, toast appears
# Press Ctrl+P: task watcher shows the backgrounded agent

# Test 4: Team creation
echo "Create a team with an Explore agent named researcher and a Planner agent named architect to analyze and plan a refactoring of the config system" | opendev -p "test team"
# Verify: team toast, watcher shows 2 members, messages flow between them

# Test 5: Task watcher navigation
# Open Ctrl+P, verify grid, test Enter for detail, Esc to close, m for messages, x for kill

# Test 6: Kill and restart
# Background an agent, press x in watcher to kill, press r to restart

# Test 7: Worktree isolation (if implemented)
echo "Spawn an agent with worktree isolation to refactor a file" | opendev -p "test worktree"
# Verify: agent works in isolated worktree, changes don't affect main tree
```

---

## Complete Hotkey Reference

### Global

| Key | Action |
|-----|--------|
| **Ctrl+B** | Background current agent (shows hint after 2s) |
| **Ctrl+P** / **Alt+B** | Toggle task watcher |
| **Esc** | Interrupt / close overlay |
| **Ctrl+C** (x2) | Quit |
| **Ctrl+O** | Toggle tool result collapse |
| **Ctrl+I** | Toggle thinking blocks |
| **Ctrl+T** | Toggle todo panel |
| **Ctrl+R** | Session picker |
| **Ctrl+D** | Debug panel |
| **Ctrl+Shift+A** | Cycle autonomy |
| **Ctrl+Shift+T** | Cycle reasoning |
| **Shift+Tab** | Toggle plan mode |

### Task Watcher

| Key | Action |
|-----|--------|
| **h/j/k/l** | Navigate cells |
| **Shift+J/K** | Scroll cell content |
| **Shift+H/L** | Page navigation |
| **Enter** | Open/close detail view |
| **m** | Open message log (team members) |
| **r** | Restart failed agent |
| **t** | Toggle sort (time/status) |
| **x** | Kill running agent |
| **q** / **Esc** | Close (detail → grid → closed) |

---

## Implementation Order

| # | What | Crate | Depends on |
|---|------|-------|-----------|
| 1 | TaskManager + types | opendev-runtime | - |
| 2 | Sidechain writer/reader | opendev-history | - |
| 3 | Enhanced SubAgentSpec | opendev-agents | - |
| 4 | TaskManager unit tests | opendev-runtime | 1 |
| 5 | Sidechain tests | opendev-history | 2 |
| 6 | BackgroundAgentManager → TaskManager adapter | opendev-tui | 1 |
| 7 | Ctrl+B hint (tick + spinner) | opendev-tui | - |
| 8 | `run_in_background` in SpawnSubagentTool | opendev-tools-impl | 1, 2, 6 (D8) |
| 8.5 | `AgentRuntime::resume_with_messages()` | opendev-cli | - (D8) |
| 9 | New SubagentEvent variants + bridge | opendev-tools-impl, opendev-cli | 8 |
| 9.5 | LLM prompt templates (team-guide, subagent-guide update) | opendev-agents | - (F1-F3) |
| 10 | New AppEvent variants + dispatch | opendev-tui | 9 |
| 11 | Background handlers + toast + E4 skip eager display | opendev-tui | 10 |
| 12 | Background integration tests | opendev-tools-impl | 8 |
| 13 | Mid-execution backgrounding enhancement | opendev-cli | 1, 2, 8.5 (D8) |
| 14 | Task watcher: detail view + enhanced footer (G2-G3) | opendev-tui | 6 |
| 15 | Task watcher: new keybindings (C1 restart) | opendev-tui | 14 |
| 16 | Mailbox system | opendev-runtime | - |
| 17 | Mailbox tests | opendev-runtime | 16 |
| 18 | TeamManager | opendev-runtime | 1, 16 |
| 19 | Team tools (Create/Send/Delete) + prompts (F4-F5) | opendev-tools-impl | 9.5, 18 |
| 20 | Teammate mailbox polling in react loop | opendev-agents | 16 |
| 21 | Team events + TUI handlers (C6 permission badge) | opendev-tui | 19 |
| 22 | Task watcher: team cells + messages + leader gold (G1) | opendev-tui | 21 |
| 23 | Status bar team pill (D7) | opendev-tui | 21 |
| 24 | Team integration tests | opendev-tools-impl | 19, 20 |
| 25 | Worktree manager | opendev-runtime | 3 |
| 26 | Worktree integration in spawn | opendev-agents | 25 |
| 27 | Full TUI test suite | opendev-tui | all |
| 28 | Real simulation tests | all | all |
| 29 | `cargo test && clippy && build` | all | all |

Steps 1, 2, 3 can be parallelized. Steps 7, 8.5, 9.5, and 16 are independent.

---

---

## ADDENDUM: Gaps Found in Revision 1 Audit (2026-03-31)

These gaps were discovered by auditing Claude Code's implementation against the plan. Each must be addressed during implementation.

### A1: Auto-Background Timeout (Missing Feature)

Claude Code auto-backgrounds foreground agents after **120 seconds** (env-gated via `CLAUDE_AUTO_BACKGROUND_TASKS`).

**Add to OpenDev**: Optional config `auto_background_timeout_secs: Option<u64>` in opendev config.

**Implementation in `tick.rs`**:
```rust
// Auto-background: if configured and foreground agent running too long
if let Some(timeout) = self.config.auto_background_timeout_secs {
    if self.state.agent_active && !self.state.backgrounding_pending {
        if let Some(start) = self.state.agent_started_at {
            if start.elapsed() > Duration::from_secs(timeout) {
                self.try_background_agent();
            }
        }
    }
}
```

**AppState addition**: `pub agent_started_at: Option<Instant>` — set in `handle_agent_started()`, cleared in `handle_agent_finished()`.

**Cancel timer**: If agent finishes naturally before timeout, no action needed (the elapsed check simply won't fire).

### A2: Rich ToolActivity Tracking (Activity Log Too Simple)

The plan uses `activity_log: Vec<String>` (plain strings). Claude Code uses typed `ToolActivity` objects:

```rust
// Replace plain String activity log with typed activities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolActivity {
    pub tool_name: String,
    pub description: String,   // "Reading src/foo.rs", "Searching for 'pattern'"
    pub is_search: bool,
    pub is_read: bool,
    pub started_at_ms: u64,
    pub finished: bool,
    pub success: bool,
}
```

**Add to TaskInfo**:
```rust
pub recent_activities: Vec<ToolActivity>,  // last 5 (MAX_RECENT_ACTIVITIES)
pub last_activity: Option<ToolActivity>,
```

**Pre-compute descriptions** at tool execution time (in BackgroundProgressCallback):
- Bash: `"Running '{first_20_chars_of_command}'"`
- Read: `"Reading {filename}"`
- Grep: `"Searching for '{pattern}'"`
- Write: `"Writing {filename}"`
- spawn_subagent: `"Spawning {agent_type}"`

### A3: Eviction Grace Period Correction

**Plan says**: 30 seconds. **Claude Code uses**: 5 seconds (`PANEL_GRACE_MS = 5000`).

**Fix**: Change `evict_after_ms` default from `now + 30_000` to `now + 5_000`.

### A4: Agent Resume from Sidechain (Missing Detail)

When `SendMessage` targets a completed agent, Claude Code **auto-resumes** it. The plan mentions resume vaguely.

**Exact resume flow for OpenDev**:
1. `SendMessageTool` checks `TaskManager.get(recipient_task_id)` state
2. If `state == Completed || state == Failed`:
   a. Open `SidechainReader` for the agent
   b. Call `into_messages()` to reconstruct message history
   c. Filter messages: remove orphaned tool calls, empty assistant messages
   d. Append the new message as a user message
   e. Create new `TaskInfo` with `state: Pending`
   f. Spawn new background task with the reconstructed + new messages
   g. Return "Agent resumed with your message"
3. If `state == Running`: queue message via `task_manager.queue_message()`

**Sidechain reader filter chain** (from Claude Code's `resumeAgentBackground`):
- `filterWhitespaceOnlyAssistantMessages()` — remove empty assistant turns
- `filterOrphanedThinkingOnlyMessages()` — remove thinking blocks with no output
- `filterUnresolvedToolUses()` — remove tool_use blocks that never got a result

### A5: Task Output File on Disk (Missing)

Claude Code writes task stdout/stderr to `~/.claude_temp/{sessionId}/tasks/{taskId}.output`. The plan has sidechain transcripts but **no separate output file**.

**Decision**: Use sidechain JSONL as the single output format. The `SidechainReader::tail(5)` method serves the same purpose as the output file for TUI preview. No separate output file needed — this simplifies the implementation.

### A6: Foregrounding a Background Task (Missing User Flow)

When user presses Enter on a task in the watcher, they should see the **live transcript**.

**Step-by-step user flow for detail view**:
1. User presses **Ctrl+P** → task watcher opens
2. User navigates to a running task with **h/j/k/l**
3. User presses **Enter** → detail view opens:
   - Header: `"{agent_type}: {description}"` with status badge
   - Stats line: `"Tools: {N} | Tokens: {N} | Cost: ${N} | Elapsed: {time}"`
   - Activity feed (scrollable with j/k):
     - For running tasks: live activity from `TaskInfo.recent_activities` + `activity_log`
     - For completed tasks: loaded from sidechain via `SidechainReader::tail(50)`
   - Footer: `"Esc:back  j/k:scroll  x:kill"`
4. While viewing, new activities stream in via `BackgroundAgentActivity` events
5. When task completes, status badge updates, toast fires
6. User presses **Esc** → returns to grid

**State for live viewing**:
```rust
// When detail view opens for a task:
task_manager.set_retain(&task_id, true);  // block eviction while viewing

// When detail view closes:
task_manager.set_retain(&task_id, false);
task_manager.set_evict_after(&task_id, now_ms() + 5_000);  // 5s grace
```

### A7: Multiple Background Agents Completing Simultaneously

**What happens**: Two agents complete at the same time while parent is busy.

**Current plan handles this**: `pending_queue` is FIFO, `drain_next_pending()` processes ONE item per call. After agent processes first result and fires `AgentFinished`, `drain_next_pending()` is called again for the second.

**But the plan should specify ordering**: Results are injected in **arrival order** (FIFO). Each result gets its own full LLM turn. No batching.

### A8: User Pressing Ctrl+C While Background Agents Run

**Missing from plan**. When user quits (Ctrl+C x2):

```rust
// In quit handler:
fn handle_quit(&mut self) {
    // Kill all running background tasks
    for task in self.task_manager.list() {
        if task.state == TaskState::Running {
            self.task_manager.kill_task(&task.task_id);
        }
    }
    // Kill all team members
    if let Some(ref team) = self.state.active_team {
        for member in &team.members {
            self.task_manager.kill_task(&member.task_id);
        }
    }
    self.state.running = false;
}
```

### A9: SendMessage Structured Message Types (Missing)

Beyond plain text, agents need structured message types for coordination:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    ShutdownRequest { reason: Option<String> },
    ShutdownResponse { approved: bool, reason: Option<String> },
    Idle { completed_task: Option<String>, status: Option<String> },
}
```

**SendMessageTool** must accept `message_type` parameter (default: "text") and serialize structured messages as JSON in the mailbox content field.

### A10: Spinner Keeps Running During Ctrl+B Hint

The plan says hint appears inline. **Clarification**: The spinner animation MUST continue running alongside the hint. The hint is appended AFTER the spinner spans, not replacing them. `shouldContinueAnimation: true` in Claude Code terms.

### A11: Eager SubagentDisplayState Creation Must Be Preserved

**Critical**: The current code creates `SubagentDisplayState` eagerly at `ToolStarted` (not `SubagentStarted`) to avoid race conditions where events arrive out of order. The `subagent_id` is initially empty and filled when `SubagentStarted` arrives later.

**The plan must NOT change this pattern**. When adding background agent support, the eager creation should be skipped for `run_in_background=true` agents (they go directly to task watcher, not conversation spinner).

### A12: dismiss_modals_for_background()

When Ctrl+B fires, any open approval/plan dialog must be auto-dismissed to unblock the react loop. This already exists in `key_handler.rs:69-99` and must be preserved.

### A13: was_killed Check Before Queuing Results

In `handle_background_agent_completed()`, the FIRST thing checked is whether the task was **already killed** before queuing. Killed task results are NOT injected into pending_queue. This pattern from the existing code must be preserved in the new TaskManager-based flow.

### A14: bg_subagent_map Cleanup on Task Completion

When a background task completes, ALL its child subagent IDs must be cleaned from `bg_subagent_map` and `subagent_cancel_tokens`. Currently done in `handle_background_agent_completed()` lines 70-100. Must be preserved.

### A15: Task Watcher Auto-Close Behavior

Current behavior: When all tasks finish, `task_watcher_all_done_at` is set. After 3 seconds of all-done state, watcher auto-closes on next tick. This should be preserved.

---

## Step-by-Step User Journey Specification

### Journey 1: Background Agent from Start (`run_in_background`)

| Step | What Happens | What User SEES | What User CAN DO |
|------|-------------|----------------|------------------|
| 1 | LLM calls `spawn_subagent(run_in_background=true)` | Inline tool result: "Background agent started. task_id: abc123" | Read result, continue conversation |
| 2 | Background task registered in TaskManager | Status bar: `"\u{2699} 1 bg (Ctrl+P)"` | Press Ctrl+P to view |
| 3 | Toast appears | Top-right: "Background agent started: {desc}" (3s) | Wait for it to fade |
| 4 | Agent runs in background | Nothing visible unless Ctrl+P pressed | Press Ctrl+P |
| 5 | Ctrl+P opens task watcher | Grid with 1 cell: spinner + agent name + activity | Navigate with h/j/k/l |
| 6 | Agent makes tool calls | Cell activity updates: `"\u{25b8} read_file"`, `"\u{2713} read_file"` | Press Enter for detail view |
| 7 | Agent completes | Cell: green check + "completed" + tool count | Toast: "Agent done: {desc}" |
| 8 | Result queued in pending_queue | If parent idle: immediately processes. If busy: waits. | Continue working |
| 9 | Parent processes result | Conversation: `[get_background_result]` tool call with summary | Respond to result |
| 10 | Task evicts after 5s | Cell disappears from watcher | Nothing |

### Journey 2: Ctrl+B Mid-Execution Backgrounding

| Step | What Happens | What User SEES | What User CAN DO |
|------|-------------|----------------|------------------|
| 1 | Agent starts running, spawn_subagent called | Conversation: spinner + "spawn_subagent" + subagent progress | Wait, scroll, type |
| 2 | 2 seconds elapse | Inline hint appears: `"  Ctrl+B to background"` (dim italic, spinner keeps running) | Press Ctrl+B or wait |
| 3 | User presses Ctrl+B | Spinner stops, tool result: "Sent to background" | Input becomes active |
| 4 | Toast fires | "Sent to background (Ctrl+P to view)" | Press Ctrl+P |
| 5 | Subagent marked as backgrounded | Disappears from conversation, appears in task watcher | Navigate watcher |
| 6 | Background task continues | Task watcher cell: running spinner + activity | Press x to kill |
| 7 | Completion | Toast + pending_queue injection (same as Journey 1, steps 7-10) | Process result |

### Journey 3: Team Workflow

| Step | What Happens | What User SEES | What User CAN DO |
|------|-------------|----------------|------------------|
| 1 | LLM calls `create_team(members=[researcher, architect])` | Tool result: "Team 'analysis' created with 2 members" | Continue conversation |
| 2 | Toast fires | "Team 'analysis' created with 2 members" | Press Ctrl+P |
| 3 | Status bar updates | `"Team:0/2 busy (Ctrl+P)"` → `"Team:2/2 busy (Ctrl+P)"` | Observe |
| 4 | Members start running | Task watcher: 2 cells with team member icons | Navigate, view detail |
| 5 | Members exchange messages | Activity lines: `"\u{25b6} To architect: findings..."` | Press m for message log |
| 6 | Member completes | Cell: green check. Toast: "[researcher] completed task" | View result in detail |
| 7 | All members complete | Both cells done. Status: `"Team:0/2 busy"` | Results injected to parent |
| 8 | LLM calls `delete_team` | Toast: "Team disbanded". Cells removed after 5s grace. | Nothing |

### Journey 4: Task Watcher Interaction

| Step | What Happens | What User SEES | What User CAN DO |
|------|-------------|----------------|------------------|
| 1 | Press Ctrl+P | Overlay opens with grid of cells | h/j/k/l to navigate |
| 2 | Navigate to a cell | Focused cell: double border | Enter for detail, x to kill |
| 3 | Press Enter | Full-screen detail view: stats + scrollable activity | j/k to scroll, Esc to back |
| 4 | Press Esc | Returns to grid view | Continue navigating |
| 5 | Press m (on team member) | Message log overlay: chronological messages | j/k to scroll, Esc to back |
| 6 | Press x (on running task) | Task killed, cell updates to "killed" | r to restart |
| 7 | Press r (on killed/failed) | New task spawned with same query, cell refreshes | Watch new execution |
| 8 | Press q or Esc (no sub-view) | Watcher closes | Back to conversation |
| 9 | All tasks complete, 3s pass | Watcher auto-closes | Nothing |

---

---

## ADDENDUM: Revision 2 Findings (2026-03-31)

### B1: `run_in_background` Agents Skip `try_background_agent()` — By Design

The existing `try_background_agent()` only recognizes `bash`, `run_command`, and `spawn_subagent` as backgroundable tools. Background agents spawned with `run_in_background=true` **never go through this path** — they are background from the start via `tokio::spawn` in `SpawnSubagentTool::execute()`.

**This is correct by design**: `try_background_agent()` is for moving a **foreground** agent to background (Ctrl+B). `run_in_background` agents are already background. No change needed.

However, we need to ensure `BackgroundAgentManager.can_accept()` is checked **inside** `SpawnSubagentTool::execute()` for `run_in_background=true`:

```rust
if run_in_background {
    if let Some(ref tm) = self.task_manager {
        if tm.running_count() >= tm.max_concurrent {
            return ToolResult::fail(format!(
                "Maximum background agents reached ({}). Wait for a task to complete or kill one.",
                tm.max_concurrent
            ));
        }
    }
    // ... proceed with background spawn
}
```

### B2: Existing Infrastructure That Doesn't Need Recreation

The audit confirmed these already exist and should be **reused, not recreated**:

| Component | Location | Plan Action |
|-----------|----------|-------------|
| `inject_background_result()` | `runtime/query.rs:419` | Reuse as-is for pending_queue drain |
| `BackgroundEventCallback` | `tui_runner/mod.rs:121-172` | Reuse pattern for background agents spawned from tool |
| `SubagentEvent` enum | `tools-impl/agents/events.rs` | Extend with new variants |
| `ChannelReceivers` struct | `runtime/mod.rs:84-90` | Extend with team event channel |
| `ToolRegistry::register()` | `tools-core/registry/mod.rs` | Use for new team tools |
| Config extensibility | `opendev-models/config/mod.rs` | Add `auto_background_timeout_secs` |

### B3: `resume_with_messages()` Doesn't Exist — Implementation Detail

The plan mentions `runtime.resume_with_messages()` for Ctrl+B mid-execution re-spawn. This method doesn't exist yet.

**Implementation approach**: Create it in `crates/opendev-cli/src/runtime/query.rs`:

```rust
/// Resume an agent from existing message history (for backgrounded agents).
/// Re-enters the react loop from the saved messages without injecting a new user turn.
pub async fn resume_with_messages(
    &self,
    messages: Vec<Value>,
    system_prompt: &str,
    callback: Option<&dyn AgentEventCallback>,
    interrupt_token: Option<&InterruptToken>,
    plan_requested: bool,
) -> Result<AgentResult, AgentError> {
    // 1. Don't prepare a new user message — messages already contain the full history
    // 2. Run ReactLoop::run() with existing messages
    // 3. The loop picks up from the last assistant message and continues
    //    (if last message was assistant with tool_calls, it executes them)
    //    (if last message was tool_result, it calls LLM for next turn)
    let mut session_messages = messages;
    // ... setup react loop config, tool schemas, etc. (same as run_query)
    self.react_loop.run(
        &self.caller, &self.http_client,
        &mut session_messages, &tool_schemas,
        &self.tool_registry, &self.tool_context,
        /* cost_tracker */ None, /* artifact_index */ None,
        /* compactor */ None, /* todo_manager */ None,
        cancel, tool_approval_tx, debug_logger,
    ).await
}
```

**Key difference from `run_query()`**: No user message injection, no history loading — just continues from where the agent left off.

### B4: Tool Registration Location for Team Tools

Team tools (`CreateTeamTool`, `SendMessageTool`, `DeleteTeamTool`) need `Arc<TaskManager>` and `Arc<TeamManager>`, which are late-binding (created after `ToolRegistry`).

**Register them in the same place as `SpawnSubagentTool`** — in `AgentRuntime::new()` at `crates/opendev-cli/src/runtime/mod.rs:360-396`:

```rust
// After SpawnSubagentTool registration (line ~396):
if let Some(ref tm) = self.task_manager {
    let team_manager = Arc::new(TeamManager::new(teams_dir, Arc::clone(tm)));
    registry.register(Arc::new(CreateTeamTool::new(
        Arc::clone(&team_manager), Arc::clone(tm),
        Arc::clone(&subagent_manager), /* ... same deps as SpawnSubagentTool */
    )));
    registry.register(Arc::new(SendMessageTool::new(
        Arc::clone(&team_manager), Arc::clone(tm),
    )));
    registry.register(Arc::new(DeleteTeamTool::new(
        Arc::clone(&team_manager), Arc::clone(tm),
    )));
}
```

### B5: Team Event Channel Extension

Add to `ToolChannelReceivers` in `crates/opendev-cli/src/runtime/mod.rs`:

```rust
pub struct ToolChannelReceivers {
    pub ask_user_rx: opendev_runtime::AskUserReceiver,
    pub plan_approval_rx: opendev_runtime::PlanApprovalReceiver,
    pub tool_approval_rx: opendev_runtime::ToolApprovalReceiver,
    pub subagent_event_rx: Option<mpsc::UnboundedReceiver<SubagentEvent>>,
    // NEW:
    pub team_event_rx: Option<mpsc::UnboundedReceiver<TeamEvent>>,
}
```

Create `TeamEvent` enum in `crates/opendev-tools-impl/src/agents/events.rs`:

```rust
#[derive(Debug, Clone)]
pub enum TeamEvent {
    Created { team_id: String, leader: String, members: Vec<String> },
    MemberStatusChanged { team_id: String, member: String, status: String },
    MessageSent { from: String, to: String, preview: String },
    Deleted { team_id: String },
}
```

Bridge in `tui_runner::run()` (same pattern as subagent bridge at lines 278-352):

```rust
if let Some(mut team_rx) = receivers.team_event_rx {
    let team_tx = event_tx.clone();
    tokio::spawn(async move {
        while let Some(evt) = team_rx.recv().await {
            let app_event = match evt {
                TeamEvent::Created { team_id, leader, members } =>
                    AppEvent::TeamCreated { team_id, leader_name: leader, member_names: members },
                TeamEvent::MemberStatusChanged { team_id, member, status } =>
                    AppEvent::TeamMemberStatusChanged { team_id, member_name: member, status },
                TeamEvent::MessageSent { from, to, preview } =>
                    AppEvent::TeamMessageSent { from, to, content_preview: preview },
                TeamEvent::Deleted { team_id } =>
                    AppEvent::TeamDeleted { team_id },
            };
            let _ = team_tx.send(app_event);
        }
    });
}
```

### B6: /clear Behavior with Background Agents

When user types `/clear`, background agents should **survive** (matching Claude Code behavior).

In the `/clear` handler (currently resets conversation messages), add:

```rust
// Preserve background tasks — don't kill them on /clear
// Only clear conversation display, not pending_queue or bg_agent_manager
self.state.messages.clear();
self.state.active_tools.clear();
// DON'T clear: pending_queue, bg_agent_manager, active_team, task_watcher state
// Background agents continue running and will deliver results when they complete
```

**Test**: `/clear` while background agent running → agent continues → result still injected when done.

### B7: Context Compaction for Background Agents

Background agents use the same `ReactLoop` as foreground agents. **Compaction works identically** — no special handling needed. The `ContextCompactor` is passed as `None` for subagents (already the case), so background agents spawned via `SubagentManager::spawn()` don't get compaction by default.

**Decision**: Keep this behavior. Background agents are typically short-lived (exploration, planning) and don't need compaction. If they hit context limits, the react loop will error naturally.

### B8: Background Agents Cannot Spawn Sub-Agents — Enforcement

Currently `ctx.is_subagent` blocks recursive spawning. But background agents spawned via `run_in_background` are NOT subagents — they run in their own context.

**Decision**: Background agents CAN spawn synchronous subagents (they have their own react loop). But they CANNOT spawn background-in-background (would orphan).

**Add check in SpawnSubagentTool::execute()**:
```rust
// Prevent background-in-background spawning
if run_in_background && ctx.values.get("is_background_agent").is_some() {
    return ToolResult::fail(
        "Background agents cannot spawn other background agents. \
         Use synchronous subagents (remove run_in_background) instead."
    );
}
```

Set the flag when spawning background task:
```rust
let mut bg_context = tool_context.clone();
bg_context.values.insert("is_background_agent".into(), json!(true));
```

### B9: Additional Tests from Revision 2

```rust
// TaskManager max concurrent enforcement for run_in_background
#[tokio::test]
async fn test_background_spawn_rejected_at_max_concurrent() {
    // Fill TaskManager to max_concurrent
    // Call execute() with run_in_background=true
    // Assert: ToolResult::fail with "Maximum background agents reached"
}

// /clear preserves background agents
#[test]
fn test_clear_preserves_background_tasks() {
    let mut app = App::new();
    // Add background task
    // Call handle_clear()
    // Assert: bg_agent_manager still has the task
    // Assert: pending_queue preserved
}

// Background-in-background prevention
#[tokio::test]
async fn test_background_in_background_rejected() {
    // Create ToolContext with is_background_agent=true
    // Call execute() with run_in_background=true
    // Assert: ToolResult::fail with "cannot spawn other background agents"
}

// Resume with messages
#[tokio::test]
async fn test_resume_continues_from_saved_messages() {
    // Create mock messages with pending tool calls
    // Call resume_with_messages()
    // Assert: tool calls get executed, result returned
}

// Team event channel bridge
#[test]
fn test_team_event_bridge_forwards_to_app_events() {
    // Send TeamEvent::Created through channel
    // Assert: AppEvent::TeamCreated received
}
```

---

## ADDENDUM: Revision 3 Findings (2026-03-31)

### C1: Restart (r key) Has No Automatic Mechanism — User Triggers Retry

Claude Code has NO automatic restart. The `r` key in the task watcher should:

```rust
fn try_restart_task(&mut self, focus_idx: usize) {
    // Get the task at focus_idx
    let task = self.get_focused_task(focus_idx);
    if task.state != TaskState::Failed && task.state != TaskState::Killed {
        return;  // Can only restart terminal tasks
    }

    // Re-queue the original query as a new background task
    // This creates a BRAND NEW task with the same agent_type + query
    let new_task_id = generate_task_id();
    // Send event to spawn a new background agent with the same parameters
    if let Some(ref tx) = self.user_message_tx {
        let restart_sentinel = format!(
            "\x00__RESTART_AGENT__{}",
            serde_json::json!({
                "agent_type": task.agent_type,
                "task": task.query,
                "description": task.description,
            })
        );
        let _ = tx.send(restart_sentinel);
    }
}
```

**In tui_runner**, add sentinel handling for `\x00__RESTART_AGENT__`:
- Parse JSON payload
- Call `runtime.run_query()` with a synthetic message: `"Spawn a background {agent_type} agent to: {task}"`
- This lets the LLM decide how to handle the restart (it may adjust approach)

### C2: Exact Cell Rendering Formats (Verified from Source)

**Background agent cell title**: `"A: {query}"` (prefix `"A: "`)
**Subagent cell title**: `"{name}: {description_or_task}"` (from `display_label()`)

**For new run_in_background agents, use a distinct prefix**:
- Async-spawned: `"BG: {description}"` (distinguish from Ctrl+B'd which use `"A: "`)
- Team member: `"T: {member_name}: {description}"`

**Footer verified format**: `"{status_str} \u{00b7} {elapsed_str} \u{00b7} {tool_count} tools"`
- Status strings: `"Working\u{2026}"`, `"Done"`, `"Failed"`, `"Killed"`

**Completion summary format** (for foreground subagents in conversation):
`"Done ({N} tool uses, {elapsed}, {tokens})"` — from `SubagentDisplayState::completion_summary()`

### C3: User Input vs Background Result Priority

When user sends a message while a background result is being processed:

**OpenDev current behavior** (verified):
- `drain_next_pending()` sets `agent_active = true` before sending sentinel
- While `agent_active = true`, user's Enter key still calls `handle_user_submit()` which pushes to `pending_queue` as `PendingItem::UserMessage`
- When background result processing finishes → `AgentFinished` → `drain_next_pending()` picks up user message next

**Result**: User input is **queued behind the current background result** but processed before subsequent background results (FIFO order preserved).

**This is correct behavior** — no change needed. Document it:

| Queue State | What Happens |
|-------------|-------------|
| `[BgResult1]` → agent processing | User types message → queue becomes `[UserMsg]` |
| BgResult1 finishes → drain | UserMsg processed next |
| UserMsg finishes → drain | Queue empty, agent idle |
| `[BgResult1, BgResult2]` → processing BgResult1 | User types → queue: `[BgResult2, UserMsg]` |
| BgResult1 finishes → drain | BgResult2 processed (FIFO) |
| BgResult2 finishes → drain | UserMsg processed |

### C4: Task Watcher Stays Open During Typing

The task watcher overlay is **modal** — when open, keyboard input goes to the watcher, not the input buffer. User must close the watcher (q/Esc) before typing.

**This is correct for a TUI** — unlike Claude Code's React UI where the watcher is a footer widget that coexists with the prompt, our ratatui overlay captures all keyboard events.

**No change needed**, but clarify in the user journey:

| Journey 4, Step 1 revised | Press Ctrl+P | Overlay opens. **Input is disabled while watcher is open.** | Navigate with h/j/k/l, press q/Esc to close and resume typing |

### C5: Background Agent Context Limit Handling

When a background agent hits context limit:
1. The `ReactLoop` returns `AgentError` (context too large)
2. `SpawnSubagentTool`'s background task catches the error
3. Calls `task_manager.fail_task(&task_id, &error.to_string())`
4. Sends `SubagentEvent::BackgroundCompleted { success: false, ... }`
5. TUI shows toast: `"Agent failed: {desc}: context limit exceeded"`
6. Result is NOT injected into pending_queue (failure handling already in plan A13)

**No auto-compaction for background agents** — they fail cleanly. This matches Claude Code's behavior (B7).

### C6: Team Member Permission Request Flow

When a team member (background agent) needs tool approval:

**Current limitation**: Background agents don't have access to `tool_approval_tx` (it's passed as `None` for subagents in `SubagentManager::spawn()`). This means background agents **auto-deny** tools requiring approval.

**For team members, we need to wire approval through**:

1. Team member's react loop encounters a tool requiring approval
2. Instead of blocking on `tool_approval_tx` (which is None), check for team context
3. If team member: send `TeamEvent::PermissionRequest { member_name, tool_name, command }` via team_event_tx
4. TUI receives event, shows approval dialog with team member badge:
   ```rust
   AppEvent::ToolApprovalRequested {
       command,
       working_dir,
       response_tx,
       // NEW field:
       agent_badge: Some(AgentBadge { name: member_name, color: team_color }),
   }
   ```
5. User approves/denies in the normal approval UI (with badge showing which agent)
6. Response sent back to team member's react loop via oneshot channel

**Implementation**: Pass `tool_approval_tx` to team member spawns (unlike regular subagents). The TUI already handles approval dialogs — just add the badge to distinguish team member requests.

**New field on `ToolApprovalRequested`** event:
```rust
AppEvent::ToolApprovalRequested {
    command: String,
    working_dir: String,
    response_tx: oneshot::Sender<ToolApprovalDecision>,
    agent_badge: Option<(String, String)>,  // (name, color) — None for main agent
}
```

**Approval dialog rendering change**: In `render_approval()`, if `agent_badge.is_some()`:
```rust
// Title bar shows: "Approval from [member_name]" instead of "Command Approval"
let title = if let Some((name, _)) = agent_badge {
    format!(" Approval from [{name}] ")
} else {
    " Command Approval ".to_string()
};
```

### C7: Notification Format for OpenDev

Claude Code uses XML notifications. For OpenDev, we use the existing `\x00__BG_RESULT__` sentinel format (JSON). No XML needed — the sentinel is already implemented and working.

**But we should enrich the payload** to match Claude Code's information density:

```json
{
    "task_id": "abc123",
    "query": "original task prompt",
    "result": "agent output text",
    "tool_call_count": 12,
    "cost_usd": 0.05,
    "input_tokens": 15000,
    "output_tokens": 3000,
    "elapsed_secs": 45,
    "agent_type": "Explore",
    "worktree_path": null,
    "worktree_branch": null
}
```

Update `drain_next_pending()` to include these extra fields in the sentinel payload, and update `inject_background_result()` to pass them through so the LLM sees full context.

### C8: Sentinel Name Correction

The plan previously mentioned `\x00__PENDING_RESULT__` — this does NOT exist. The correct sentinel is `\x00__BG_RESULT__`. All references in the plan already use the correct name. Confirmed.

### C9: Additional Tests from Revision 3

```rust
// Restart creates new task
#[test]
fn test_restart_creates_new_task_with_same_query() {
    let mut app = App::new();
    // Add failed task
    // Call try_restart_task()
    // Assert: sentinel sent to user_message_tx
    // Assert: original task unchanged (still failed)
}

// User input queued behind background result
#[test]
fn test_user_input_queued_behind_active_bg_result() {
    let mut app = App::new();
    app.state.agent_active = true;  // processing bg result
    app.handle_user_submit("user message");
    assert_eq!(app.state.pending_queue.len(), 1);
    assert!(matches!(app.state.pending_queue[0], PendingItem::UserMessage(_)));
}

// Team permission request shows badge
#[test]
fn test_team_permission_shows_agent_badge() {
    let mut app = App::new();
    app.handle_event(AppEvent::ToolApprovalRequested {
        command: "rm -rf".into(),
        working_dir: ".".into(),
        response_tx: tx,
        agent_badge: Some(("researcher".into(), "cyan".into())),
    });
    // Assert: approval_controller.agent_badge is Some
}

// Context limit in background agent
#[tokio::test]
async fn test_background_agent_context_limit_fails_cleanly() {
    // Setup: mock agent that returns context limit error
    // Spawn background
    // Assert: task state = Failed
    // Assert: error message contains "context"
    // Assert: no pending_queue injection
}

// FIFO ordering of mixed queue
#[test]
fn test_pending_queue_fifo_mixed_items() {
    let mut app = App::new();
    app.state.pending_queue.push_back(PendingItem::BackgroundResult { .. });
    app.state.pending_queue.push_back(PendingItem::UserMessage("hello".into()));
    app.state.pending_queue.push_back(PendingItem::BackgroundResult { .. });
    // First drain: BackgroundResult
    app.drain_next_pending();
    assert!(matches!(app.state.pending_queue.front(), Some(PendingItem::UserMessage(_))));
}
```

---

## ADDENDUM: Revision 4 — Cross-Cutting Concerns (2026-03-31)

### D1: `background_task_count` Is Computed, Not Stored — New Tasks Must Contribute

`background_task_count` is **dynamically computed every tick** in `tick.rs:155-189` from THREE sources:

```
bg_agent_running (from bg_agent_manager.running_count())
+ bg_process_running (from task_manager shell tasks)
+ bg_subagent_running (from active_subagents where backgrounded && !finished)
- covered_bg_count (dedup: parent bg_task IDs with active subagent children)
```

**For new `run_in_background` agents**: They're registered in `bg_agent_manager` via `SetBackgroundAgentToken` event, so `bg_agent_manager.running_count()` already includes them. No extra wiring needed.

**For team members**: They'll also be registered in `bg_agent_manager` (via the same `SetBackgroundAgentToken` pattern), so they contribute to the count automatically.

**No code change needed** — the existing tick computation handles everything as long as new tasks are registered in `bg_agent_manager`.

### D2: Config Flows via AgentRuntime, NOT AppState

`auto_background_timeout_secs` lives in `AppConfig` (opendev-models), accessed via `AgentRuntime.config`. But the TUI's `handle_tick()` runs on `App`, which doesn't have direct config access.

**Solution**: Pass the timeout value to AppState during initialization:

```rust
// In tui_runner::run(), before starting event loop:
app.state.auto_background_timeout_secs = self.runtime.config.auto_background_timeout_secs;
```

**Add to AppState**:
```rust
pub auto_background_timeout_secs: Option<u64>,  // None = disabled
```

**Add to AppConfig** (opendev-models/src/config/mod.rs):
```rust
#[serde(default)]
pub auto_background_timeout_secs: Option<u64>,  // None = disabled, Some(120) = 2 min
```

### D3: Schema Is Dynamic — No Caching Issue

`parameter_schema()` is called fresh every time `get_schemas()` is invoked. Adding `run_in_background` to the schema works immediately with no cache invalidation needed.

### D4: PendingItem::BackgroundResult Enrichment

The existing `PendingItem::BackgroundResult` has 6 fields. Per C7, we want to add more. Since this is our own type, just extend it:

```rust
pub enum PendingItem {
    UserMessage(String),
    BackgroundResult {
        task_id: String,
        query: String,
        result: String,
        success: bool,
        tool_call_count: usize,
        cost_usd: f64,
        // NEW fields:
        input_tokens: u64,
        output_tokens: u64,
        elapsed_secs: u64,
        agent_type: String,
    },
}
```

Update `drain_next_pending()` to include new fields in the sentinel JSON. Update `inject_background_result()` to accept and forward them.

### D5: SubagentEvent Channel Creation Pattern for Team Events

Team events follow the exact same pattern as subagent events (verified in `runtime/mod.rs:376-397`):

```rust
// In runtime/mod.rs, after subagent channel creation:
let (team_event_tx, team_event_rx) =
    tokio::sync::mpsc::unbounded_channel::<opendev_tools_impl::TeamEvent>();

// Pass sender to team tools:
registry.register(Arc::new(CreateTeamTool::new(...).with_event_sender(team_event_tx.clone())));
registry.register(Arc::new(SendMessageTool::new(...).with_event_sender(team_event_tx.clone())));
registry.register(Arc::new(DeleteTeamTool::new(...).with_event_sender(team_event_tx)));

// Store receiver:
channel_receivers.team_event_rx = Some(team_event_rx);
```

### D6: handle_agent_started() Cleans Up Subagents — Interaction with Background

`handle_agent_started()` calls `active_subagents.retain(|s| !s.finished || s.backgrounded)`. This means:
- Finished foreground subagents are removed on next agent start (correct)
- Backgrounded subagents are preserved (correct)
- **New `run_in_background` agents never enter `active_subagents`** (they go directly to bg_agent_manager), so this cleanup doesn't affect them (correct)

No change needed.

### D7: Status Bar Enhancement — Exact Integration Point

Current format: `"\u{2699} N (Ctrl+P)"` with optional spinner.

For team status, append after the existing background task section in `status_bar.rs:241-264`:

```rust
// After existing background_tasks section:
if let Some((busy, total)) = self.team_status {
    spans.push(Span::styled("  \u{2502}  ", Style::default().fg(GREY)));
    spans.push(Span::styled(
        format!("Team:{busy}/{total}"),
        Style::default().fg(style_tokens::CYAN).bold(),
    ));
}
```

**Pass from render.rs** (after line 219):
```rust
.team_status(self.state.active_team.as_ref().map(|t| {
    let busy = t.members.iter().filter(|m| m.status == TeamMemberStatus::Busy).count();
    (busy, t.members.len())
}))
```

### D8: Implementation Sequence Dependency Validation

Verified all 29 steps have correct dependencies. One ordering refinement:

**Step 8 (`run_in_background` in SpawnSubagentTool) also needs Step 6** (BackgroundAgentManager adapter) because the background spawn calls `bg_agent_manager.can_accept()` via TaskManager. Updated dependency: Step 8 depends on Steps 1, 2, **and 6**.

**Step 13 (mid-execution backgrounding) also needs `resume_with_messages()`** which is a new runtime method. Add dependency: Step 13 depends on Steps 1, 2, **and a new Step 8.5**: create `AgentRuntime::resume_with_messages()`.

### D9: Plan Maturity Assessment

After 4 revision rounds with 42 total findings (A1-A15, B1-B9, C1-C9, D1-D8):

**Fully specified**: TaskManager, Sidechain, Background spawn, SubAgentSpec, Mailbox, TeamManager, Event wiring, Keybindings, Toasts, Status bar, Pending queue, Sentinel format, Permission flow

**Implementation-ready with code**: SpawnSubagentTool background mode, TaskInfo struct, ToolActivity struct, Cell rendering, Auto-close logic, Restart mechanism, Resume method, Channel creation

**Edge cases covered**: Idempotent kills, duplicate notifications, max concurrent, context limits, /clear preservation, Ctrl+C cleanup, background-in-background prevention, modal dismissal, user input priority, FIFO ordering

**The plan is complete.** No further revision cycles needed unless the user identifies specific gaps.

---

## ADDENDUM: Revision 5 — Cross-Feature Interaction Verification (2026-03-31)

### E1: Task Watcher Grid Ordering with Mixed Types

Verified from source (`background_tasks.rs:216-236`). Grid renders in this exact order:

1. **Backgrounded subagents** (from `self.subagents` filtered by `backgrounded=true`)
2. **Background agent tasks** (from `bg_agent_manager`, filtered by `!covered`)

"Covered" means a bg_agent_task whose child subagents are already shown individually. These are hidden to avoid duplication.

**For our new types, the ordering will be**:
```
Position 0..N:  Backgrounded subagents (Ctrl+B'd)
Position N..M:  Background agent tasks from bg_agent_manager, which includes:
                - run_in_background agents (was_async_spawn=true)
                - Team members (team_id is set)
```

Team members and async agents are stored in `bg_agent_manager` via `SetBackgroundAgentToken`, so they naturally appear in the second section. **No ordering change needed** — the existing merge logic handles all types.

### E2: Failed BackgroundResult Still Injected to LLM

Verified: `drain_next_pending()` processes both `success=true` and `success=false` results identically — both create a `DisplayMessage` and inject the sentinel. The LLM sees the failure and can decide how to respond (retry, report to user, etc.).

**This is correct behavior.** The plan's A13 ("was_killed check before queuing") only prevents injection for **killed** tasks, not failed ones. Failed results should reach the LLM so it can adapt.

### E3: Ctrl+B While Task Watcher Open — Closes Watcher First

Verified: `key_handler.rs:878-893` checks `task_watcher_open` first and returns early (closes watcher). The user must press Ctrl+B **twice** — once to close watcher, once to background.

**This is the correct TUI behavior** — modal overlays should dismiss before other actions fire. No change needed, but document clearly:

> **Tip**: To background an agent while viewing the task watcher, press **Ctrl+B** (closes watcher) then **Ctrl+B** again (backgrounds agent).

### E4: `run_in_background` Tool Result Handling — No DisplayMessage Created

Verified: When `spawn_subagent` returns immediately for `run_in_background=true`, the tool result is a normal string like `"Background agent started. task_id: abc123"`. This goes through the standard `ToolResult` path in `handle_tools.rs` and creates a normal `DisplayMessage` with the result text.

However, the **subagent display** is skipped — no eager `SubagentDisplayState` is created because the subagent goes directly to `bg_agent_manager` via `SetBackgroundAgentToken`.

**Implementation detail**: In `handle_tool_started()`, when `tool_name == "spawn_subagent"` and `args["run_in_background"] == true`, **skip the eager SubagentDisplayState creation**:

```rust
// In handle_tools.rs, handle_tool_started():
if tool_name == "spawn_subagent" {
    let run_in_bg = args.get("run_in_background")
        .and_then(|v| v.as_bool()).unwrap_or(false);
    if !run_in_bg {
        // Existing eager creation logic (only for foreground subagents)
        let mut sa = SubagentDisplayState::new(...);
        sa.parent_tool_id = Some(tool_id.clone());
        self.state.active_subagents.push(sa);
    }
    // For run_in_background, skip — the agent goes to bg_agent_manager instead
}
```

This prevents a phantom subagent spinner in the conversation for agents that are background from the start.

### E5: bg_subagent_map Only Populated for Backgrounded Subagents

Verified: `bg_subagent_map.insert()` only happens in `handle_subagent_started()` when the subagent is routed to a running background task. Foreground subagents never enter the map.

**For `run_in_background` agents**: Their child subagents (if the background agent spawns sync subagents) will be routed via the same mechanism — `handle_subagent_started()` finds the running bg_task via `pending_spawn_count` and maps accordingly. **No change needed.**

### E6: Final Completeness Summary

| Area | Status | Revisions |
|------|--------|-----------|
| TaskManager lifecycle | Complete | A3, D1, D8 |
| Sidechain transcripts | Complete | A4, A5 |
| Background spawn | Complete | B1, B2, B8, D3, D4, E4 |
| Mid-execution backgrounding | Complete | A10, A11, A12, B3, E3 |
| Enhanced SubAgentSpec | Complete | Phase 4 |
| Mailbox system | Complete | Phase 5A |
| Team system | Complete | B4, B5, C6, D5, D7 |
| Worktree isolation | Complete | Phase 6 |
| TUI event wiring | Complete | B5, D1, D2, D5, D6 |
| Task watcher display | Complete | C2, E1, E4 |
| Keybindings | Complete | C1, C4, E3 |
| Toast notifications | Complete | Phase 7K |
| Status bar | Complete | D7 |
| Pending queue | Complete | A7, C3, E2 |
| Error handling | Complete | A8, A13, B8, C5 |
| Testing | Complete | A-series, B9, C9, 90+ tests |
| User journeys | Complete | 4 journeys, 34 steps |

**45 total findings across 5 revisions. Plan is stable and implementation-ready.**

---

## ADDENDUM: Revision 6 — LLM-Facing Tool Guidance (2026-03-31)

### F1: New Tools Need System Prompt Guidance (Critical for Usability)

The LLM learns tool behavior from **two sources**: JSON schemas (what tools exist) and system prompt templates (when/how to use them). Registering tools in `ToolRegistry` makes them callable, but without prompt guidance the LLM won't know when to use them.

**New prompt templates to create**:

#### `crates/opendev-agents/templates/system/main/main-team-guide.md`
```markdown
## Agent Teams

You can create teams of agents that work together on complex tasks.

### When to use teams
- Tasks requiring multiple specialized agents working in parallel
- Tasks where agents need to share findings (researcher + implementer)
- Complex analysis requiring different perspectives (explorer + planner + reviewer)

### How teams work
1. Call `create_team` with a team name and list of members (agent type + task for each)
2. Members run as background agents and communicate via `send_message`
3. Members' results are delivered to you when they complete
4. Call `delete_team` when done to clean up

### Tools
- `create_team`: Create a named team with N member agents. Each member runs in background.
- `send_message`: Send a message to a specific team member (by name) or broadcast to all ("*").
- `delete_team`: Shut down all team members and clean up.

### Example
To analyze a large codebase, create a team:
- "researcher" (Explore agent): explores crate structure and key files
- "architect" (Planner agent): designs the refactoring approach based on researcher's findings

Members communicate findings via send_message. You receive their results when they complete.
```

#### Update `crates/opendev-agents/templates/system/main/main-subagent-guide.md`
Add this section:
```markdown
### Background Agents
Use `run_in_background: true` when spawning an agent for a long-running task.
The agent runs in background — you receive a task_id immediately and get notified
when it completes. This lets you continue working on other tasks.

When to use background agents:
- Long exploration tasks (>30 seconds expected)
- Tasks where you don't need the result immediately
- When spawning multiple independent agents

Example: spawn 3 background Explore agents to analyze different parts of the codebase
simultaneously, then process their results as they come in.
```

#### Update `crates/opendev-agents/templates/system/main/main-available-tools.md`
Add to the tool categories list:
```markdown
- **Teams**: create_team, send_message, delete_team
```

### F2: Tool Descriptions Must Be Self-Sufficient

Each tool's `description()` method is the primary source the LLM reads. These must be comprehensive:

#### CreateTeamTool description
```rust
fn description(&self) -> &str {
    "Create a team of agents that work together. Each member runs as a background \
     agent with its own task. Members communicate via send_message. \
     Use for complex tasks requiring multiple specialized agents in parallel."
}
```

#### SendMessageTool description
```rust
fn description(&self) -> &str {
    "Send a message to a team member by name, or broadcast to all members with to=\"*\". \
     Use to coordinate work, share findings, or request actions from teammates. \
     If the recipient has completed, sending a message will resume it."
}
```

#### DeleteTeamTool description
```rust
fn description(&self) -> &str {
    "Shut down a team. Sends shutdown requests to all members, waits briefly for \
     graceful exit, then kills remaining. Cleans up team files."
}
```

#### Updated SpawnSubagentTool description (add `run_in_background` mention)
```rust
fn description(&self) -> &str {
    "Spawn a subagent to handle an isolated task. The subagent runs its own \
     ReAct loop with restricted tools and returns the result. Use for tasks \
     that require multiple tool calls and benefit from isolated context. \
     Set run_in_background=true for long-running tasks — returns a task_id \
     immediately and notifies you when complete."
}
```

### F3: Prompt Template Registration

System prompt sections are loaded via `PromptComposer` with priority ordering. The new team guide needs to be registered:

**In `crates/opendev-agents/src/prompts/composer/factories.rs`**:
```rust
// After the subagent guide section (~priority 45):
sections.push(PromptSection {
    name: "team-guide",
    priority: 46,  // Right after subagent guide
    content: load_template("system/main/main-team-guide.md"),
    conditional: true,  // Only include when team tools are registered
});
```

**Conditional inclusion**: Only add the team guide section when `CreateTeamTool` is in the registry. Check via `tool_registry.has_tool("create_team")`.

### F4: Team Member System Prompt Enhancement

When team members are spawned, their system prompt should include team context. In `CreateTeamTool::execute()`, prepend to each member's task:

```rust
let team_context = format!(
    "[TEAM CONTEXT]\n\
     You are '{member_name}', a member of team '{team_name}'.\n\
     Team leader: the agent that created this team.\n\
     Teammates: {other_member_names}\n\
     \n\
     Use send_message to communicate findings to teammates or the leader.\n\
     You will receive messages from teammates injected before your LLM calls.\n\
     If you receive a SHUTDOWN REQUEST, wrap up and call task_complete.\n\
     \n\
     [YOUR TASK]\n\
     {original_task}"
);
```

### F5: Tool Filtering for Team Members

Team members need access to `send_message` in addition to their normal tool set. In `CreateTeamTool`, when spawning members:

```rust
// Ensure team tools are in the member's tool set
let mut member_tools = spec.tools.clone();
if !member_tools.is_empty() {
    // If tools are restricted, add send_message explicitly
    if !member_tools.contains(&"send_message".to_string()) {
        member_tools.push("send_message".to_string());
    }
}
```

This ensures that even agents with restricted tool lists (like Explore with only read tools) can still communicate with teammates.

### F6: Updated Finding Count

**51 total findings across 6 revisions (A1-A15, B1-B9, C1-C9, D1-D8, E1-E6, F1-F5).**

---

## ADDENDUM: Revision 7 — Render Pattern Verification (2026-03-31)

### G1: TaskCellData Abstraction — No New Widget Needed

Verified from source (`background_tasks.rs:470-664`): Both `build_subagent_cell()` and `build_bg_agent_cell()` produce the same `TaskCellData` struct, which `render_cell()` renders uniformly. The cell rendering handles:
- Title: `" {icon} {title} "` with truncation
- Border: double for focused, plain for normal; color by state (green=done, red=failed, grey=running)
- Activity: 3-segment lines (icon|verb|args) with scroll + "N more" indicator
- Footer: `" {status} · {elapsed} · {N} tools "` with state-colored text

**For new task types** (async spawn, team members), we do NOT need new render functions. Just produce `TaskCellData` with different `title` prefixes:
- Async spawn: `title = format!("BG: {}", description)` instead of `"A: {query}"`
- Team member: `title = format!("T/{}: {}", member_name, description)`
- Team leader: same render, but override `border_color` to GOLD in `render_cell()` when `team_id.is_some() && is_leader`

**One small change to `render_cell()`** for team leader distinction:
```rust
// In render_cell(), before border_color computation:
let is_team_leader = /* check from TaskCellData or a new bool field */;
let border_color = if data.is_focused {
    if is_team_leader { style_tokens::GOLD } else { style_tokens::FOCUS_BORDER }
} else if /* ... existing logic ... */
```

### G2: Enhanced Footer via TaskCellData

The plan calls for tokens/cost/msgs in the footer. Since `TaskCellData.footer` is a plain `String`, just format it richer:

```rust
// In build_bg_agent_cell() for new task types:
let mut footer_parts = vec![
    status_str.to_string(),
    elapsed_str,
    format!("{} tools", task.tool_call_count),
];
if task.input_tokens + task.output_tokens > 0 {
    footer_parts.push(format!("{}tok", format_compact(task.input_tokens + task.output_tokens)));
}
if task.cost_usd > 0.001 {
    footer_parts.push(format!("${:.3}", task.cost_usd));
}
if let Some(msg_count) = task.message_count() {
    if msg_count > 0 { footer_parts.push(format!("{}msgs", msg_count)); }
}
let footer = footer_parts.join(" · ");
```

No changes to `render_cell()` needed — it just renders `data.footer` as-is.

### G3: Detail View Rendering (Enter Key)

The detail view (E4/A6) renders as a full-size overlay replacing the grid. Reuse the same `render_cell()` approach but with:
- `area` = entire task watcher overlay area (not a grid cell)
- `scroll_offset` controlled by j/k keys
- Additional stats rendered above the activity feed

```rust
fn render_task_detail(&self, frame: &mut Frame, area: Rect, idx: usize) {
    let task = self.get_task_at(idx);

    // Header (2 lines)
    let header = format!("{} — {}", task.agent_type, task.description);
    let stats = format!(
        "Tools: {} | Tokens: {} | Cost: ${:.3} | Elapsed: {}",
        task.tool_call_count,
        format_compact(task.input_tokens + task.output_tokens),
        task.cost_usd,
        format_elapsed(task.elapsed_secs()),
    );

    // Activity feed (scrollable, fills remaining space)
    // Reuse parse_activity_line() for formatting

    // Footer help text
    let help = " Esc:back  j/k:scroll  x:kill ";
}
```

### G4: No Further Gaps Found

After verifying the actual render code, cell construction, and border/chrome logic, the plan's TUI design integrates cleanly with the existing `TaskCellData` abstraction. No new widgets or rendering primitives are needed — only new `build_*_cell()` functions that produce `TaskCellData` with appropriate titles, footers, and activity lines.

**53 total findings across 7 revisions. Plan is stable and complete.**

---

## Post-Change Verification

```bash
cargo test --workspace --lib --tests
cargo clippy --workspace -- -D warnings
cargo check --workspace
cargo fmt --all
cargo build --release -p opendev-cli
echo "hello" | opendev -p "hello"
cargo clean --profile dev
```
