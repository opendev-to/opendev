//! Subagent specifications and execution.

pub mod custom_loader;
pub mod manager;
pub mod spec;

pub use manager::{
    NoopProgressCallback, SubagentManager, SubagentProgressCallback, SubagentRunResult,
    SubagentType,
};
pub use spec::{SubAgentSpec, builtins};
