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

pub mod doom_loop;
pub mod llm_calls;
pub mod main_agent;
pub mod prompts;
pub mod react_loop;
pub mod response;
pub mod skills;
pub mod subagents;
pub mod traits;

pub use doom_loop::{DoomLoopAction, DoomLoopDetector, RecoveryAction};
pub use llm_calls::LlmCaller;
pub use main_agent::MainAgent;
pub use prompts::{PromptComposer, PromptSection};
pub use react_loop::{IterationMetrics, ReactLoop, ReactLoopConfig, ToolCallMetric, TurnResult};
pub use response::ResponseCleaner;
pub use skills::{LoadedSkill, SkillLoader, SkillMetadata, SkillSource};
pub use subagents::{
    NoopProgressCallback, SubAgentSpec, SubagentManager, SubagentProgressCallback,
    SubagentRunResult, SubagentType,
};
pub use traits::{
    AgentDeps, AgentError, AgentEventCallback, AgentResult, BaseAgent, LlmResponse, TaskMonitor,
};
