//! Approval rules system for pattern-based command approval.
//!
//! Rules can be session-only (ephemeral) or persistent across sessions.
//! Persistent rules are stored in:
//!   - User-global: `~/.opendev/permissions.json`
//!   - Project-scoped: `.opendev/permissions.json`
//!
//! Ported from `opendev/core/runtime/approval/rules.py`.

mod manager;
mod persistence;
mod types;

pub use manager::ApprovalRulesManager;
pub use types::{ApprovalRule, CommandHistory, RuleAction, RuleScope, RuleType};

#[cfg(test)]
mod tests;
