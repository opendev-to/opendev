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
pub mod constants;
pub mod cost_tracker;
pub mod custom_commands;
pub mod debug_logger;
pub mod error_handler;
pub mod errors;
pub mod event_bus;
pub mod gitignore;
pub mod interrupt;
pub mod plan_index;
pub mod plan_names;
pub mod session_model;
pub mod snapshot;
pub mod sound;
pub mod todo;

// Re-export key types at crate root for convenience.
pub use approval::{ApprovalRule, ApprovalRulesManager, RuleAction, RuleScope, RuleType};
pub use constants::{AutonomyLevel, SAFE_COMMANDS, ThinkingLevel, is_safe_command};
pub use cost_tracker::{CostTracker, PricingInfo, TokenUsage};
pub use error_handler::{ErrorAction, ErrorResult, OperationError};
pub use errors::{ErrorCategory, StructuredError, classify_api_error};
pub use interrupt::{InterruptToken, InterruptedError};
pub use plan_index::PlanIndex;
pub use plan_names::generate_plan_name;
pub use session_model::SessionModelManager;
pub use todo::{TodoItem, TodoManager, TodoStatus, parse_plan_steps, parse_status, strip_markdown};

pub use action_summarizer::summarize_action;
pub use custom_commands::{CustomCommand, CustomCommandLoader};
pub use debug_logger::SessionDebugLogger;
pub use event_bus::{Event, EventBus, FilteredSubscriber};
pub use gitignore::GitIgnoreParser;
pub use snapshot::SnapshotManager;
pub use sound::play_finish_sound;
