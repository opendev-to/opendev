//! State types for nested subagent tool display.

use std::collections::HashMap;
use std::time::Instant;

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
        }
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
        self.tool_call_count = tool_call_count;
        self.shallow_warning = shallow_warning;
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
                "search" | "list_files" | "find_symbol" | "find_referencing_symbols" => {
                    search_count += 1
                }
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
mod tests {
    use super::*;

    #[test]
    fn test_subagent_display_state_new() {
        let state = SubagentDisplayState::new("id-1".into(), "Explore".into(), "Find TODOs".into());
        assert_eq!(state.name, "Explore");
        assert!(!state.finished);
        assert_eq!(state.tool_call_count, 0);
    }

    #[test]
    fn test_add_and_complete_tool_call() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        assert_eq!(state.tool_call_count, 1);
        assert!(state.active_tools.contains_key("tc-1"));

        state.complete_tool_call("tc-1", true);
        assert!(state.active_tools.is_empty());
        assert_eq!(state.completed_tools.len(), 1);
        assert!(state.completed_tools[0].success);
    }

    #[test]
    fn test_finish() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.finish(true, "Done".into(), 3, None);
        assert!(state.finished);
        assert!(state.success);
        assert_eq!(state.result_summary, "Done");
        assert_eq!(state.tool_call_count, 3);
    }

    #[test]
    fn test_finish_with_shallow_warning() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.finish(
            true,
            "Done".into(),
            1,
            Some("Shallow subagent warning".into()),
        );
        assert!(state.shallow_warning.is_some());
    }

    #[test]
    fn test_advance_tick() {
        let mut state = SubagentDisplayState::new("id-test".into(), "test".into(), "task".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        state.advance_tick();
        assert_eq!(state.tick, 1);
        assert_eq!(state.active_tools["tc-1"].tick, 1);
    }

    #[test]
    fn test_token_accumulation() {
        let mut state = SubagentDisplayState::new("id-tok".into(), "test".into(), "task".into());
        assert_eq!(state.token_count, 0);
        state.add_tokens(1000, 500);
        assert_eq!(state.token_count, 1500);
        state.add_tokens(2000, 300);
        assert_eq!(state.token_count, 3800);
    }

    #[test]
    fn test_completed_tools_cap() {
        let mut state = SubagentDisplayState::new("id-cap".into(), "test".into(), "task".into());
        // Add 150 tool calls and complete them all
        for i in 0..150 {
            let id = format!("tc-{i}");
            state.add_tool_call("read_file".into(), id.clone(), HashMap::new());
            state.complete_tool_call(&id, true);
        }
        // Should be capped at 100
        assert_eq!(state.completed_tools.len(), 100);
        assert_eq!(state.tool_call_count, 150);
    }

    #[test]
    fn test_activity_summary_reading() {
        let mut state = SubagentDisplayState::new("id-act".into(), "test".into(), "task".into());
        state.add_tool_call("read_file".into(), "tc-1".into(), HashMap::new());
        state.add_tool_call("read_file".into(), "tc-2".into(), HashMap::new());
        state.add_tool_call("read_file".into(), "tc-3".into(), HashMap::new());
        assert_eq!(state.activity_summary(), "Reading 3 files...");
    }

    #[test]
    fn test_activity_summary_searching() {
        let mut state = SubagentDisplayState::new("id-act2".into(), "test".into(), "task".into());
        state.add_tool_call("search".into(), "tc-1".into(), HashMap::new());
        state.add_tool_call("list_files".into(), "tc-2".into(), HashMap::new());
        assert_eq!(state.activity_summary(), "Searching for 2 patterns...");
    }

    #[test]
    fn test_activity_summary_running() {
        let state = SubagentDisplayState::new("id-act3".into(), "test".into(), "task".into());
        assert_eq!(state.activity_summary(), "Running...");
    }

    #[test]
    fn test_activity_summary_done() {
        let mut state = SubagentDisplayState::new("id-act4".into(), "test".into(), "task".into());
        state.finish(true, "Done".into(), 0, None);
        assert_eq!(state.activity_summary(), "Done");
    }
}
