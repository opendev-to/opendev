//! SubAgent specification types.
//!
//! Mirrors `opendev/core/agents/subagents/specs.py`.

mod builder;
pub mod builtins;
mod mode;
mod permissions;
mod types;

pub use mode::AgentMode;
pub use permissions::{PermissionAction, PermissionRule};
pub(crate) use permissions::{glob_match, pattern_specificity};
pub use types::{AgentPermissionMode, IsolationMode, SubAgentSpec};
