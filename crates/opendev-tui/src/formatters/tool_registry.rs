//! Centralized tool display registry — single source of truth for how tools appear in the TUI.
//!
//! This module re-exports from focused submodules:
//! - `tool_categories` — enums (`ToolCategory`, `ResultFormat`), the `ToolDisplayEntry` struct,
//!   and simple lookup helpers (`categorize_tool`, `tool_color`, `tool_display_parts`).
//! - `tool_entries` — the static `TOOL_REGISTRY` array, fallback entries, runtime display map,
//!   and `lookup_tool()` resolution logic.
//! - `tool_call_format` — formatting functions that turn tool names + args into display strings.

// Re-export everything that was previously public from this module.
pub use super::tool_call_format::{
    GREEN_GRADIENT, format_tool_call_display, format_tool_call_parts,
    format_tool_call_parts_short, format_tool_call_parts_with_wd,
};
pub use super::tool_categories::{
    ResultFormat, ToolCategory, ToolDisplayEntry, categorize_tool, tool_color, tool_display_parts,
};
pub use super::tool_entries::{init_runtime_display, lookup_tool};

#[cfg(test)]
#[path = "tool_registry_tests.rs"]
mod tests;
