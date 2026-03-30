//! Agent layer for OpenDev.
//!
//! This crate provides:
//! - [`traits`] — `BaseAgent` async trait, `AgentResult`, `LlmResponse`, `AgentDeps`
//! - [`main_agent`] — `MainAgent` struct using composition
//! - [`llm_calls`] — `LlmCaller` for different LLM call types
//! - [`react_loop`] — ReAct loop: reason → decide tool → execute → observe → loop
//! - [`prompts`] — `PromptComposer` with priority-ordered conditional sections
//! - [`subagents`] — Subagent definitions and manager
//! - [`response`] — Response cleaning and normalization
//! - [`skills`] — Lazy-loaded knowledge modules (markdown with frontmatter)
//! - [`agent_types`] — Agent definitions, handoff protocol, parallel tool grouping

pub mod agent_types;
pub mod doom_loop;
pub mod llm_calls;
pub mod main_agent;
pub mod prompts;
pub mod react_loop;
pub mod response;
pub mod skills;
pub mod subagents;
pub mod traits;

pub use agent_types::{AgentDefinition, AgentRole, HandoffMessage, PartialResult, can_parallelize};
pub use doom_loop::{DoomLoopAction, DoomLoopDetector, RecoveryAction};
pub use llm_calls::LlmCaller;
pub use main_agent::MainAgent;
pub use prompts::{PromptComposer, PromptSection};
pub use react_loop::{IterationMetrics, ReactLoop, ReactLoopConfig, ToolCallMetric, TurnResult};
pub use response::ResponseCleaner;
pub use skills::{LoadedSkill, SkillLoader, SkillMetadata, SkillSource};
pub use subagents::{
    NoopProgressCallback, PermissionAction, PermissionRule, RunnerContext, SimpleReactRunner,
    StandardReactRunner, SubAgentSpec, SubagentEventBridge, SubagentManager,
    SubagentProgressCallback, SubagentRunResult, SubagentRunner, SubagentType,
};
pub use traits::{
    AgentDeps, AgentError, AgentEventCallback, AgentResult, BaseAgent, LlmResponse, TaskMonitor,
};

/// Resolve the git repository root for a working directory.
///
/// Returns `None` if the directory is not inside a git repository.
/// This is a shared helper to avoid redundant `git rev-parse --show-toplevel` calls.
pub fn git_root(working_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(working_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8(o.stdout)
                .ok()
                .map(|s| std::path::PathBuf::from(s.trim()))
        })
}
