//! Core data models for the OpenDev AI coding assistant.
//!
//! This crate defines all shared data types used across the system:
//! messages, sessions, configuration, file changes, operations, users,
//! and API request/response models.

pub mod api;
pub mod config;
pub mod datetime_compat;
pub mod file_change;
pub mod frontend_event;
pub mod message;
pub mod operation;
pub mod session;
pub mod user;
pub mod validator;

// Re-export commonly used types at crate root
pub use config::{
    AgentConfigInline, AppConfig, AutoModeConfig, ModelVariant, OperationConfig, PermissionConfig,
    PlaybookConfig, PlaybookScoringWeights, ToolPermission,
};
pub use file_change::{FileChange, FileChangeType};
pub use message::{ChatMessage, InputProvenance, ProvenanceKind, Role, ToolCall};
pub use operation::{
    BashResult, EditResult, Operation, OperationStatus, OperationType, WriteResult,
};
pub use session::{Session, SessionMetadata};
pub use user::User;
