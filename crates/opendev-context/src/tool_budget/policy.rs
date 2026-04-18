//! Per-tool size caps for `apply_tool_result_budget`.
//!
//! Conservative caps for known-noisy tools (wide listings, search hits)
//! and generous caps for tools whose value is in their full content
//! (file reads). `usize::MAX` opts a tool out of budgeting entirely
//! (e.g. binary content like screenshots).

use crate::compaction::TOOL_RESULT_BUDGET_DEFAULT_CHARS;

/// Built-in per-tool overrides. Names cover both the snake_case and
/// PascalCase variants OpenDev's tool registry uses for the same
/// underlying capability (`read_file` and `Read`, `Bash` and `run_command`,
/// etc.) so the policy applies symmetrically regardless of caller.
const TOOL_BUDGET_OVERRIDES: &[(&str, usize)] = &[
    // File reads: full content is the value.
    ("read_file", 12_000),
    ("Read", 12_000),
    // Search results compress well with truncation.
    ("Grep", 4_000),
    ("search", 4_000),
    ("file_search", 4_000),
    // Wide directory listings are noisy.
    ("list_files", 2_000),
    ("list_directory", 2_000),
    ("List", 2_000),
    // Shell output: enough to see the tail but cap runaway logs.
    ("Bash", 6_000),
    ("run_command", 6_000),
    ("bash_execute", 6_000),
    // Binary / opaque content: do not budget.
    ("web_screenshot", usize::MAX),
    ("vlm", usize::MAX),
];

/// Per-tool character cap policy.
#[derive(Debug, Clone)]
pub struct ToolBudgetPolicy {
    default_chars: usize,
    overrides: Vec<(String, usize)>,
}

impl Default for ToolBudgetPolicy {
    fn default() -> Self {
        Self {
            default_chars: TOOL_RESULT_BUDGET_DEFAULT_CHARS,
            overrides: TOOL_BUDGET_OVERRIDES
                .iter()
                .map(|(k, v)| ((*k).to_string(), *v))
                .collect(),
        }
    }
}

impl ToolBudgetPolicy {
    /// Construct a policy with the built-in overrides and a custom default.
    pub fn with_default_chars(default_chars: usize) -> Self {
        Self {
            default_chars,
            ..Self::default()
        }
    }

    /// Override the cap for a single tool. Pass `usize::MAX` to opt out.
    pub fn set_override(&mut self, tool_name: impl Into<String>, cap: usize) {
        let tool_name = tool_name.into();
        if let Some(slot) = self.overrides.iter_mut().find(|(k, _)| *k == tool_name) {
            slot.1 = cap;
        } else {
            self.overrides.push((tool_name, cap));
        }
    }

    /// Return the character cap that applies to `tool_name`. Falls back
    /// to the configured default when no override matches.
    pub fn cap_for(&self, tool_name: &str) -> usize {
        self.overrides
            .iter()
            .find(|(k, _)| k == tool_name)
            .map(|(_, v)| *v)
            .unwrap_or(self.default_chars)
    }

    /// Returns true if `tool_name` is opted out of budgeting (cap == MAX).
    pub fn is_unbounded(&self, tool_name: &str) -> bool {
        self.cap_for(tool_name) == usize::MAX
    }
}
