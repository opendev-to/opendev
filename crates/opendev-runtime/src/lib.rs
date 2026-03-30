//! Runtime services for the OpenDev AI coding assistant.
//!
//! This crate provides:
//! - [`approval`] — Pattern-based command approval rules with persistence
//! - [`cost_tracker`] — Session-level token usage and cost tracking
//! - [`interrupt`] — Async-safe cancellation token (CancellationToken pattern)
//! - [`plan_index`] — Plan-session-project association index (JSON CRUD)
//! - [`plan_names`] — Unique plan name generation (adjective-verb-noun)
//! - [`session_model`] — Per-session model configuration overlay
//! - [`error_handler`] — Error classification, retry logic, user-facing recovery
//! - [`errors`] — Structured error types with provider pattern matching

pub mod action_summarizer;
pub mod approval;
pub mod ask_user_channel;
pub mod constants;
pub mod cost_tracker;
pub mod custom_commands;
pub mod debug_logger;
pub mod directory_context;
pub mod error_handler;
pub mod errors;
pub mod event_bus;
pub mod file_watcher;
pub mod gitignore;
pub mod interrupt;
pub mod lazy_init;
pub mod permissions;
pub mod plan_approval;
pub mod plan_index;
pub mod plan_names;
pub mod sandbox;
pub mod secrets;
pub mod session_model;
pub mod session_status;
pub mod snapshot;
pub mod sound;
pub mod state_snapshot;
pub mod task_scheduler;
pub mod todo;
pub mod tool_approval_channel;
pub mod tool_summarizer;

// Re-export key types at crate root for convenience.
pub use approval::{ApprovalRule, ApprovalRulesManager, RuleAction, RuleScope, RuleType};
pub use constants::{AutonomyLevel, SAFE_COMMANDS, extract_command_prefix, is_safe_command};
pub use cost_tracker::{CostTracker, PricingInfo, TokenUsage};
pub use error_handler::{ErrorAction, ErrorResult, OperationError};
pub use errors::{ErrorCategory, StructuredError, classify_api_error};
pub use interrupt::{InterruptToken, InterruptedError};
pub use plan_index::PlanIndex;
pub use plan_names::generate_plan_name;
pub use session_model::SessionModelManager;
pub use todo::{
    SubTodoItem, TodoItem, TodoManager, TodoStatus, parse_plan_steps, parse_status, strip_markdown,
};

pub use action_summarizer::summarize_action;
pub use ask_user_channel::{AskUserReceiver, AskUserRequest, AskUserSender, ask_user_channel};
pub use custom_commands::{CustomCommand, CustomCommandLoader};
pub use debug_logger::SessionDebugLogger;
pub use event_bus::{
    Event, EventBus, EventTopic, FilteredSubscriber, RuntimeEvent, TopicSubscriber,
    create_event_bus_bridge, group_runtime_events_by_topic,
};
pub use file_watcher::{FileChange, FileChangeKind, FileWatcher, FileWatcherConfig};
pub use gitignore::GitIgnoreParser;
pub use lazy_init::{
    LazyEmbeddings, LazyLsp, LazyMcp, LazySubsystem, SyncLazy, create_lazy_subsystems,
};
pub use permissions::{PermissionAction, PermissionRule, PermissionRuleSet, is_sensitive_file};
pub use plan_approval::{
    PlanApprovalReceiver, PlanApprovalRequest, PlanApprovalSender, PlanDecision,
    plan_approval_channel,
};
pub use sandbox::SandboxConfig;
pub use secrets::{SecretKind, SecretMatch, detect_secrets, redact_secrets};
pub use session_status::{SessionStatus, SessionStatusTracker};
pub use snapshot::SnapshotManager;
pub use sound::play_finish_sound;
pub use state_snapshot::{AppStateSnapshot, SnapshotPersistence, ToolResultEntry};
pub use task_scheduler::TaskScheduler;
pub use tool_approval_channel::{
    ToolApprovalDecision, ToolApprovalReceiver, ToolApprovalRequest, ToolApprovalSender,
    tool_approval_channel,
};
pub use tool_summarizer::{build_background_result, safe_truncate, summarize_tool_result};
