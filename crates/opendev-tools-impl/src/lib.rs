//! Tool implementations for OpenDev.
//!
//! Each module implements the `BaseTool` trait from `opendev-tools-core`.
//! Tools are grouped by function:
//!
//! - File operations: [`file_read`], [`file_write`], [`file_edit`], [`file_list`], [`file_search`]
//! - Process execution: [`bash`]
//! - Git operations: [`git`]
//! - Web tools: [`web_fetch`], [`web_search`], [`web_screenshot`], [`browser`]
//! - User interaction: [`ask_user`]
//! - Memory tools: [`memory`]
//! - Session tools: [`session`]
//! - Patch application: [`patch`]
//! - Scheduling: [`schedule`]
//! - PDF extraction: [`pdf`]
//! - Browser opening: [`open_browser`]
//! - Agent management: [`agents`]
//! - Batch execution: [`batch`]
//! - Diff preview: [`diff_preview`]
//! - Messaging: [`message`]
//! - Notebook editing: [`notebook_edit`]
//! - Task completion: [`task_complete`]
//! - Vision LM: [`vlm`]
//! - Plan presentation: [`present_plan`]
//! - Todo management: [`write_todos`], [`update_todo`], [`complete_todo`], [`list_todos`], [`clear_todos`]

pub mod agents;
pub mod ask_user;
pub mod bash;
pub mod batch;
pub mod browser;
pub mod clear_todos;
pub mod complete_todo;
pub mod diff_preview;
pub mod edit_replacers;
pub mod file_edit;
pub mod file_list;
pub mod file_read;
pub mod file_search;
pub mod file_write;
pub mod formatter;
pub mod git;
pub mod invoke_skill;
pub mod list_todos;
pub mod memory;
pub mod message;
pub mod multi_edit;
pub mod notebook_edit;
pub mod open_browser;
pub mod patch;
pub mod path_utils;
pub mod present_plan;
pub mod schedule;
pub mod session;
pub mod task_complete;
pub mod todo;
pub mod update_todo;
pub mod vlm;
pub mod web_fetch;
pub mod web_screenshot;
pub mod web_search;
pub mod worktree;
pub mod write_todos;

/// Re-export all tool structs for convenient registration.
pub use agents::{AgentsTool, ChannelProgressCallback, SpawnSubagentTool, SubagentEvent};
pub use ask_user::AskUserTool;
pub use bash::BashTool;
pub use batch::BatchTool;
pub use browser::BrowserTool;
pub use clear_todos::ClearTodosTool;
pub use complete_todo::CompleteTodoTool;
pub use diff_preview::DiffPreviewTool;
pub use file_edit::FileEditTool;
pub use file_list::FileListTool;
pub use file_read::FileReadTool;
pub use file_search::FileSearchTool;
pub use file_write::FileWriteTool;
pub use git::GitTool;
pub use invoke_skill::InvokeSkillTool;
pub use list_todos::ListTodosTool;
pub use memory::MemoryTool;
pub use message::MessageTool;
pub use multi_edit::MultiEditTool;
pub use notebook_edit::NotebookEditTool;
pub use open_browser::OpenBrowserTool;
pub use patch::PatchTool;
pub use present_plan::PresentPlanTool;
pub use schedule::ScheduleTool;
pub use session::SessionTool;
pub use task_complete::TaskCompleteTool;
pub use todo::TodoTool;
pub use update_todo::UpdateTodoTool;
pub use vlm::VlmTool;
pub use web_fetch::WebFetchTool;
pub use web_screenshot::WebScreenshotTool;
pub use web_search::WebSearchTool;
pub use worktree::{WorktreeInfo, WorktreeManager};
pub use write_todos::WriteTodosTool;
