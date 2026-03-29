//! Handler for bash/process execution.
//!
//! Mirrors `opendev/core/context_engineering/tools/handlers/process_handlers.py`.
//!
//! Responsibilities:
//! - Detect server/long-running commands and promote to background
//! - Check command approval rules
//! - Truncate output for display
//! - Track operations for audit

use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;
use tracing::debug;

use opendev_runtime::approval::ApprovalRulesManager;

use super::traits::{HandlerMeta, HandlerResult, PreCheckResult, ToolHandler};

/// Maximum output lines before truncation.
const MAX_OUTPUT_LINES: usize = 200;

/// Patterns that indicate a server/long-running process.
const SERVER_PATTERNS: &[&str] = &[
    r"flask\s+run",
    r"python.*app\.py",
    r"python.*manage\.py\s+runserver",
    r"django.*runserver",
    r"uvicorn",
    r"gunicorn",
    r"python.*-m\s+http\.server",
    r"npm\s+(run\s+)?(start|dev|serve)",
    r"yarn\s+(run\s+)?(start|dev|serve)",
    r"node.*server",
    r"nodemon",
    r"next\s+(dev|start)",
    r"rails\s+server",
    r"php.*artisan\s+serve",
    r"hugo\s+server",
    r"jekyll\s+serve",
    r"cargo\s+run",
    r"go\s+run",
];

/// Handler for bash/process tool execution.
pub struct ProcessHandler {
    approval_manager: Option<ApprovalRulesManager>,
    server_re: Vec<Regex>,
}

impl ProcessHandler {
    /// Create a new process handler.
    pub fn new(approval_manager: Option<ApprovalRulesManager>) -> Self {
        let server_re = SERVER_PATTERNS
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        Self {
            approval_manager,
            server_re,
        }
    }

    /// Check if a command looks like a server/long-running process.
    fn is_server_command(&self, command: &str) -> bool {
        self.server_re.iter().any(|re| re.is_match(command))
    }

    /// Truncate output to MAX_OUTPUT_LINES, preserving head and tail.
    fn truncate_output(output: &str) -> String {
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() <= MAX_OUTPUT_LINES {
            return output.to_string();
        }

        let head = MAX_OUTPUT_LINES * 2 / 3;
        let tail = MAX_OUTPUT_LINES - head - 1;
        let omitted = lines.len() - head - tail;

        let mut result = lines[..head].join("\n");
        result.push_str(&format!("\n\n... ({omitted} lines omitted) ...\n\n"));
        result.push_str(&lines[lines.len() - tail..].join("\n"));
        result
    }
}

impl ToolHandler for ProcessHandler {
    fn handles(&self) -> &[&str] {
        &["Bash", "bash"]
    }

    fn pre_check(&self, _tool_name: &str, args: &HashMap<String, Value>) -> PreCheckResult {
        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(cmd) => cmd,
            None => return PreCheckResult::Deny("Missing 'command' argument".to_string()),
        };

        // Check approval rules if manager is available.
        if let Some(ref mgr) = self.approval_manager
            && let Some(rule) = mgr.evaluate_command(command)
        {
            use opendev_runtime::approval::RuleAction;
            match rule.action {
                RuleAction::AutoDeny => {
                    return PreCheckResult::Deny(format!(
                        "Command denied by rule: {}",
                        &rule.description,
                    ));
                }
                RuleAction::AutoApprove => {
                    debug!(command, "Command auto-approved by rule");
                }
                _ => {} // RequireApproval, RequireEdit handled upstream
            }
        }

        // Auto-promote server commands to background.
        let background = args
            .get("background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !background && self.is_server_command(command) {
            debug!(command, "Auto-promoting server command to background");
            let mut new_args = args.clone();
            new_args.insert("background".to_string(), Value::Bool(true));
            return PreCheckResult::ModifyArgs(new_args);
        }

        PreCheckResult::Allow
    }

    fn post_process(
        &self,
        _tool_name: &str,
        args: &HashMap<String, Value>,
        output: Option<&str>,
        error: Option<&str>,
        success: bool,
    ) -> HandlerResult {
        let is_background = args
            .get("background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let truncated_output = output.map(Self::truncate_output);

        HandlerResult {
            output: truncated_output,
            error: error.map(|s| s.to_string()),
            success,
            meta: HandlerMeta {
                is_background,
                operation_id: Some(format!(
                    "bash_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0)
                )),
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
#[path = "process_handler_tests.rs"]
mod tests;
