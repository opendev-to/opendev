//! Subagent specifications and execution.

pub mod custom_loader;
pub mod manager;
pub mod runner;
pub mod spec;

pub use manager::{
    NoopProgressCallback, SubagentEventBridge, SubagentManager, SubagentProgressCallback,
    SubagentRunResult, SubagentType,
};
pub use runner::{RunnerContext, SimpleReactRunner, StandardReactRunner, SubagentRunner};
pub use spec::{
    AgentMode, AgentPermissionMode, IsolationMode, PermissionAction, PermissionRule, SubAgentSpec,
    builtins,
};
