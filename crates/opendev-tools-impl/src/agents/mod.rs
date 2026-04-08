//! Agents tool — list and spawn subagent types.
//!
//! Provides two tools:
//! - `agents` — List available subagent configurations
//! - `spawn_subagent` — Spawn a subagent to handle an isolated task
//!
//! Mirrors `opendev/core/context_engineering/tools/implementations/agents_tool.py`
//! and the subagent spawning logic from the Python react loop.

pub mod check_mailbox;
mod events;
mod list;
mod spawn;
pub mod spawn_teammate;
pub mod task_list_tools;
pub mod team_tools;
pub mod tool_search;

pub use check_mailbox::CheckMailboxTool;
pub use events::{ChannelProgressCallback, SubagentEvent};
pub use list::AgentsTool;
pub use spawn::SpawnSubagentTool;
pub use spawn_teammate::SpawnTeammateTool;
pub use task_list_tools::{
    TeamAddTaskTool, TeamClaimTaskTool, TeamCompleteTaskTool, TeamListTasksTool,
};
pub use team_tools::{CreateTeamTool, DeleteTeamTool, SendMessageTool};
pub use tool_search::ToolSearchTool;
