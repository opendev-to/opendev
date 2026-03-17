//! Agent type definitions for specialized agent roles.
//!
//! Provides `AgentDefinition` for configuring different agent types (Code, Plan,
//! Test, Build) with distinct system prompts, thinking levels, tool sets, and
//! optional per-agent model overrides for thinking/critique phases.

mod coordination;
mod definitions;
mod roles;

pub use coordination::{HandoffMessage, PartialResult, can_parallelize};
pub use definitions::AgentDefinition;
pub use roles::AgentRole;
