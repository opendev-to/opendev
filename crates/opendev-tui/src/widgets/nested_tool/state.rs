//! State types for nested subagent tool display.

use std::collections::HashMap;
use std::time::Instant;

use crate::formatters::tool_line::format_elapsed;

/// Format a token count as human-readable (e.g., "1.2k tokens", "3.5M tokens").
fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens} tokens")
    }
}

/// State tracking for a single subagent execution.
#[derive(Debug, Clone)]
pub struct SubagentDisplayState {
    /// Unique identifier for this subagent instance.
    pub subagent_id: String,
    /// Subagent name.
    pub name: String,
    /// Task description.
    pub task: String,
    /// When the subagent started.
    pub started_at: Instant,
    /// Whether the subagent has finished.
    pub finished: bool,
    /// Whether the subagent succeeded (only valid when finished).
    pub success: bool,
    /// Final result summary (only valid when finished).
    pub result_summary: String,
    /// Total tool calls made.
    pub tool_call_count: usize,
    /// Active tool calls (tool_id -> NestedToolCallState).
    pub active_tools: HashMap<String, NestedToolCallState>,
    /// Completed tool calls (for display).
    pub completed_tools: Vec<CompletedToolCall>,
    /// Accumulated token count (input + output).
    pub token_count: u64,
    /// Animation tick counter for spinner.
    pub tick: usize,
    /// Optional shallow subagent warning.
    pub shallow_warning: Option<String>,
    /// When the subagent finished (for cleanup timing).
    pub finished_at: Option<Instant>,
    /// The parent spawn_subagent tool_id (set when SubagentStarted is linked to ToolStarted).
    pub parent_tool_id: Option<String>,
    /// Short display label (from `description` param), used instead of full task in spinner.
    pub description: Option<String>,
    /// Whether this subagent's parent was sent to background (Ctrl+B).
    pub backgrounded: bool,
}

impl SubagentDisplayState {
    /// Create a new subagent display state.
    pub fn new(subagent_id: String, name: String, task: String) -> Self {
        Self {
            subagent_id,
            name,
            task,
            started_at: Instant::now(),
            finished: false,
            success: false,
            result_summary: String::new(),
            tool_call_count: 0,
            active_tools: HashMap::new(),
            completed_tools: Vec::new(),
            token_count: 0,
            tick: 0,
            shallow_warning: None,
            finished_at: None,
            parent_tool_id: None,
            description: None,
            backgrounded: false,
        }
    }

    /// Returns the short display label if set, otherwise the full task.
    pub fn display_label(&self) -> &str {
        self.description.as_deref().unwrap_or(&self.task)
    }

    /// Record a new tool call starting.
    pub fn add_tool_call(
        &mut self,
        tool_name: String,
        tool_id: String,
        args: HashMap<String, serde_json::Value>,
    ) {
        self.tool_call_count += 1;
        self.active_tools.insert(
            tool_id.clone(),
            NestedToolCallState {
                tool_name,
                tool_id,
                args,
                started_at: Instant::now(),
                tick: 0,
            },
        );
    }

    /// Accumulate token usage from an LLM call.
    pub fn add_tokens(&mut self, input_tokens: u64, output_tokens: u64) {
        self.token_count += input_tokens + output_tokens;
    }

    /// Record a tool call completing.
    pub fn complete_tool_call(&mut self, tool_id: &str, success: bool) {
        if let Some(state) = self.active_tools.remove(tool_id) {
            self.completed_tools.push(CompletedToolCall {
                tool_name: state.tool_name,
                args: state.args,
                elapsed: state.started_at.elapsed(),
                success,
            });
            // Cap completed tools to prevent unbounded growth in long-running subagents
            if self.completed_tools.len() > 100 {
                self.completed_tools
                    .drain(..self.completed_tools.len() - 100);
            }
        }
    }

    /// Mark the subagent as finished.
    pub fn finish(
        &mut self,
        success: bool,
        result_summary: String,
        tool_call_count: usize,
        shallow_warning: Option<String>,
    ) {
        self.finished = true;
        self.finished_at = Some(Instant::now());
        self.success = success;
        self.result_summary = result_summary;
        self.tool_call_count = tool_call_count.max(self.tool_call_count);
        self.shallow_warning = shallow_warning;
        // Clear tool lists to reduce visual clutter — keep only header with "Done" status
        self.active_tools.clear();
        self.completed_tools.clear();
    }

    /// Advance the animation tick.
    pub fn advance_tick(&mut self) {
        self.tick += 1;
        for tool in self.active_tools.values_mut() {
            tool.tick += 1;
        }
    }

    /// Elapsed time since start.
    pub fn elapsed_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Generate a persistent completion summary for display after the subagent finishes.
    /// Format: "Done (N tool uses, Xs, N.Nk tokens)"
    pub fn completion_summary(&self) -> String {
        let mut parts = Vec::new();

        let tc = self.tool_call_count;
        parts.push(if tc == 1 {
            "1 tool use".to_string()
        } else {
            format!("{tc} tool uses")
        });

        let elapsed = self.started_at.elapsed().as_secs();
        parts.push(format_elapsed(elapsed));

        if self.token_count > 0 {
            parts.push(format_token_count(self.token_count));
        }

        format!("Done ({})", parts.join(", "))
    }

    /// Generate a summary of current activity.
    pub fn activity_summary(&self) -> String {
        if self.active_tools.is_empty() {
            if self.finished {
                return "Done".to_string();
            }
            return "Running...".to_string();
        }

        // Count active tool types
        let mut read_count = 0usize;
        let mut search_count = 0usize;
        let mut other_name = None;
        for tool in self.active_tools.values() {
            match tool.tool_name.as_str() {
                "read_file" | "read_pdf" => read_count += 1,
                "grep"
                | "ast_grep"
                | "search"
                | "list_files"
                | "find_symbol"
                | "find_referencing_symbols" => search_count += 1,
                name => other_name = Some(name.to_string()),
            }
        }

        if read_count > 1 {
            format!("Reading {} files...", read_count)
        } else if search_count > 1 {
            format!("Searching for {} patterns...", search_count)
        } else if let Some(name) = other_name {
            format!("{name}...")
        } else if read_count == 1 {
            "Reading...".to_string()
        } else {
            "Running...".to_string()
        }
    }
}

/// State for an active nested tool call.
#[derive(Debug, Clone)]
pub struct NestedToolCallState {
    pub tool_name: String,
    pub tool_id: String,
    pub args: HashMap<String, serde_json::Value>,
    pub started_at: Instant,
    pub tick: usize,
}

/// Record of a completed tool call.
#[derive(Debug, Clone)]
pub struct CompletedToolCall {
    pub tool_name: String,
    pub args: HashMap<String, serde_json::Value>,
    pub elapsed: std::time::Duration,
    pub success: bool,
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
