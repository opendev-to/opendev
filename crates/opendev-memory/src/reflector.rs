//! Execution reflector for extracting learnable patterns from tool executions.
//!
//! Mirrors `opendev/core/context_engineering/memory/reflection/reflector.py`.

use std::collections::HashMap;

/// Result of reflecting on a tool execution sequence.
#[derive(Debug, Clone)]
pub struct ReflectionResult {
    pub category: String,
    pub content: String,
    pub confidence: f64,
    pub reasoning: String,
}

/// Lightweight representation of a tool call for reflection analysis.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub parameters: HashMap<String, String>,
}

/// Analyzes tool execution sequences to extract learnable patterns.
///
/// Identifies patterns worth learning from tool executions,
/// such as multi-step workflows, error recovery, and best practices.
pub struct ExecutionReflector {
    pub min_tool_calls: usize,
    pub min_confidence: f64,
}

impl ExecutionReflector {
    /// Create a new execution reflector.
    pub fn new(min_tool_calls: usize, min_confidence: f64) -> Self {
        Self {
            min_tool_calls,
            min_confidence,
        }
    }

    /// Extract a reusable strategy from tool execution.
    pub fn reflect(
        &self,
        _query: &str,
        tool_calls: &[ToolCallInfo],
        outcome: &str,
    ) -> Option<ReflectionResult> {
        if !self.is_worth_learning(tool_calls, outcome) {
            return None;
        }

        let result = self
            .extract_file_operation_pattern(tool_calls)
            .or_else(|| self.extract_code_navigation_pattern(tool_calls))
            .or_else(|| self.extract_testing_pattern(tool_calls))
            .or_else(|| self.extract_shell_command_pattern(tool_calls))
            .or_else(|| self.extract_error_recovery_pattern(tool_calls, outcome));

        result.filter(|r| r.confidence >= self.min_confidence)
    }

    fn is_worth_learning(&self, tool_calls: &[ToolCallInfo], outcome: &str) -> bool {
        // Always learn from error recovery
        if outcome == "error" && !tool_calls.is_empty() {
            return true;
        }

        // Skip single trivial operations
        if tool_calls.len() == 1 {
            let name = &tool_calls[0].name;
            if name == "read_file" || name == "list_files" {
                return false;
            }
        }

        // Learn from multi-step sequences
        if tool_calls.len() >= self.min_tool_calls {
            return true;
        }

        false
    }

    fn extract_file_operation_pattern(
        &self,
        tool_calls: &[ToolCallInfo],
    ) -> Option<ReflectionResult> {
        let names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();

        // Pattern: list_files -> read_file
        if let (Some(list_idx), Some(read_idx)) = (
            names.iter().position(|&n| n == "list_files"),
            names.iter().position(|&n| n == "read_file"),
        ) && list_idx < read_idx
        {
            return Some(ReflectionResult {
                category: "file_operations".to_string(),
                content: "List directory contents before reading files to understand \
                              structure and locate files"
                    .to_string(),
                confidence: 0.75,
                reasoning: "Sequential list_files -> read_file pattern shows exploratory \
                                file access"
                    .to_string(),
            });
        }

        // Pattern: read_file -> write_file
        if let (Some(read_idx), Some(write_idx)) = (
            names.iter().position(|&n| n == "read_file"),
            names.iter().position(|&n| n == "write_file"),
        ) && read_idx < write_idx
        {
            return Some(ReflectionResult {
                category: "file_operations".to_string(),
                content: "Read file contents before writing to understand current state \
                              and preserve important data"
                    .to_string(),
                confidence: 0.8,
                reasoning: "Sequential read_file -> write_file shows safe modification \
                                workflow"
                    .to_string(),
            });
        }

        // Pattern: multiple read_file calls
        let read_count = names.iter().filter(|&&n| n == "read_file").count();
        if read_count >= 3 {
            return Some(ReflectionResult {
                category: "code_navigation".to_string(),
                content: "When understanding complex code, read multiple related files to \
                          build complete picture"
                    .to_string(),
                confidence: 0.7,
                reasoning: format!(
                    "Multiple file reads ({read_count}) indicates thorough code exploration"
                ),
            });
        }

        None
    }

    fn extract_code_navigation_pattern(
        &self,
        tool_calls: &[ToolCallInfo],
    ) -> Option<ReflectionResult> {
        let names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();

        // Pattern: search -> read_file
        if let (Some(search_idx), Some(read_idx)) = (
            names.iter().position(|&n| n == "search"),
            names.iter().position(|&n| n == "read_file"),
        ) && search_idx < read_idx
        {
            return Some(ReflectionResult {
                category: "code_navigation".to_string(),
                content: "Search for keywords or patterns before reading files to locate \
                              relevant code efficiently"
                    .to_string(),
                confidence: 0.8,
                reasoning: "Search followed by read shows targeted file access".to_string(),
            });
        }

        // Pattern: multiple searches
        let search_count = names.iter().filter(|&&n| n == "search").count();
        if search_count >= 2 {
            return Some(ReflectionResult {
                category: "code_navigation".to_string(),
                content: "Use multiple searches with different keywords to thoroughly explore \
                          codebase and find all relevant locations"
                    .to_string(),
                confidence: 0.7,
                reasoning: format!(
                    "Multiple searches ({search_count}) shows iterative code exploration"
                ),
            });
        }

        None
    }

    fn extract_testing_pattern(&self, tool_calls: &[ToolCallInfo]) -> Option<ReflectionResult> {
        let names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();

        let test_keywords = ["test", "pytest", "jest", "npm test"];
        let has_test_command = tool_calls.iter().any(|tc| {
            tc.name == "run_command"
                && tc.parameters.get("command").is_some_and(|cmd| {
                    let lower = cmd.to_lowercase();
                    test_keywords.iter().any(|kw| lower.contains(kw))
                })
        });

        if !has_test_command {
            return None;
        }

        if names.contains(&"write_file") || names.contains(&"edit_file") {
            return Some(ReflectionResult {
                category: "testing".to_string(),
                content: "Run tests after making code changes to verify correctness and \
                          catch regressions early"
                    .to_string(),
                confidence: 0.85,
                reasoning: "Code modification followed by test execution shows good \
                            development practice"
                    .to_string(),
            });
        }

        None
    }

    fn extract_shell_command_pattern(
        &self,
        tool_calls: &[ToolCallInfo],
    ) -> Option<ReflectionResult> {
        let commands: Vec<&str> = tool_calls
            .iter()
            .filter(|tc| tc.name == "run_command")
            .filter_map(|tc| tc.parameters.get("command").map(String::as_str))
            .collect();

        if commands.len() < 2 {
            return None;
        }

        let install_keywords = [
            "npm install",
            "pip install",
            "yarn install",
            "poetry install",
        ];
        let run_keywords = ["npm start", "python", "node", "pytest"];

        let has_install = commands.iter().any(|cmd| {
            install_keywords
                .iter()
                .any(|kw| cmd.to_lowercase().contains(kw))
        });
        let has_run = commands.iter().any(|cmd| {
            run_keywords
                .iter()
                .any(|kw| cmd.to_lowercase().contains(kw))
        });

        if has_install && has_run {
            return Some(ReflectionResult {
                category: "shell_commands".to_string(),
                content: "Install dependencies before running or testing applications to \
                          ensure all requirements are met"
                    .to_string(),
                confidence: 0.8,
                reasoning: "Install followed by run/test shows proper setup workflow".to_string(),
            });
        }

        if commands.iter().any(|cmd| cmd.contains("git status")) {
            return Some(ReflectionResult {
                category: "git_operations".to_string(),
                content: "Check git status before performing git operations to understand \
                          current state and avoid mistakes"
                    .to_string(),
                confidence: 0.75,
                reasoning: "Git status check before operations shows careful version control \
                            practice"
                    .to_string(),
            });
        }

        None
    }

    fn extract_error_recovery_pattern(
        &self,
        tool_calls: &[ToolCallInfo],
        outcome: &str,
    ) -> Option<ReflectionResult> {
        if outcome != "error" {
            return None;
        }

        let names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();

        if names.contains(&"read_file") {
            return Some(ReflectionResult {
                category: "error_handling".to_string(),
                content: "When file access fails, list directory first to verify file exists \
                          and check path correctness"
                    .to_string(),
                confidence: 0.7,
                reasoning: "File access error suggests need for directory verification".to_string(),
            });
        }

        if names.contains(&"run_command") {
            return Some(ReflectionResult {
                category: "error_handling".to_string(),
                content: "When commands fail, verify environment setup, dependencies, and \
                          working directory before retrying"
                    .to_string(),
                confidence: 0.65,
                reasoning: "Command execution error suggests environment or dependency issue"
                    .to_string(),
            });
        }

        None
    }
}

impl Default for ExecutionReflector {
    fn default() -> Self {
        Self::new(2, 0.6)
    }
}

/// Default recency decay factor per day.
const RECENCY_DECAY: f64 = 0.95;

/// Score a reflection by evidence count and recency.
///
/// The score combines evidence strength with temporal decay:
///   `score = evidence_count * recency_decay^age_days`
///
/// - More evidence (observations supporting the reflection) increases the score.
/// - Older reflections decay exponentially, encouraging fresh insights.
/// - A reflection with zero evidence scores 0.0 regardless of age.
///
/// # Arguments
/// * `_reflection` - The reflection text (reserved for future content-based scoring).
/// * `evidence_count` - Number of supporting observations.
/// * `age_days` - How many days old the reflection is.
pub fn score_reflection(_reflection: &str, evidence_count: usize, age_days: u64) -> f64 {
    if evidence_count == 0 {
        return 0.0;
    }
    let decay = RECENCY_DECAY.powi(age_days as i32);
    evidence_count as f64 * decay
}

#[cfg(test)]
#[path = "reflector_tests.rs"]
mod tests;
