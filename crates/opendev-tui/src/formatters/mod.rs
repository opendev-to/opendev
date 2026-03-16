//! Output formatters for terminal rendering.

pub mod base;
pub mod bash_formatter;
pub mod directory_formatter;
pub mod display;
pub mod factory;
pub mod file_formatter;
pub mod generic_formatter;
pub mod markdown;
pub mod style_tokens;
pub mod todo_formatter;
pub mod tool_registry;

pub use base::{FormattedOutput, ToolFormatter};
pub use display::{
    format_error, format_info, format_warning, strip_system_reminders, truncate_output,
};
pub use factory::FormatterFactory;
pub use markdown::MarkdownRenderer;
pub use tool_registry::{
    GREEN_GRADIENT, ToolCategory, categorize_tool, format_tool_call_display,
    format_tool_call_parts, format_tool_call_parts_with_wd, tool_color, tool_display_parts,
};
