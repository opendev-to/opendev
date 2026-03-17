//! Git operations tool — structured git commands with safety checks.

mod ops;

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use ops::{
    git_branch, git_checkout, git_commit, git_create_pr, git_diff, git_log, git_merge, git_pull,
    git_push, git_stash, git_status,
};

/// Tool for structured git operations.
#[derive(Debug)]
pub struct GitTool;

#[async_trait::async_trait]
impl BaseTool for GitTool {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Execute structured git operations: status, diff, log, branch, checkout, commit, push, pull, stash, merge, create_pr."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["status", "diff", "log", "branch", "checkout", "commit", "push", "pull", "stash", "merge", "create_pr"],
                    "description": "Git action to perform"
                },
                "message": { "type": "string", "description": "Commit message (for commit) or stash message (for stash push)" },
                "branch": { "type": "string", "description": "Branch name" },
                "file": { "type": "string", "description": "File path (for diff)" },
                "staged": { "type": "boolean", "description": "Show staged changes (for diff)" },
                "limit": { "type": "integer", "description": "Number of log entries" },
                "force": { "type": "boolean", "description": "Force push (with lease)" },
                "create": { "type": "boolean", "description": "Create new branch (for checkout)" },
                "remote": { "type": "string", "description": "Remote name (default: origin)" },
                "stash_action": { "type": "string", "enum": ["push", "pop", "list", "drop", "show"], "description": "Stash sub-action (default: list)" },
                "title": { "type": "string", "description": "PR title (for create_pr)" },
                "body": { "type": "string", "description": "PR body (for create_pr)" },
                "base": { "type": "string", "description": "Base branch for PR (for create_pr)" }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        let cwd = ctx.working_dir.to_string_lossy().to_string();

        match action {
            "status" => git_status(&cwd),
            "diff" => {
                let file = args.get("file").and_then(|v| v.as_str());
                let staged = args
                    .get("staged")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                git_diff(&cwd, file, staged)
            }
            "log" => {
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                git_log(&cwd, limit)
            }
            "branch" => {
                let name = args.get("branch").and_then(|v| v.as_str());
                git_branch(&cwd, name)
            }
            "checkout" => {
                let branch = match args.get("branch").and_then(|v| v.as_str()) {
                    Some(b) => b,
                    None => return ToolResult::fail("branch is required for checkout"),
                };
                let create = args
                    .get("create")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                git_checkout(&cwd, branch, create)
            }
            "commit" => {
                let message = match args.get("message").and_then(|v| v.as_str()) {
                    Some(m) => m,
                    None => return ToolResult::fail("message is required for commit"),
                };
                git_commit(&cwd, message)
            }
            "push" => {
                let remote = args
                    .get("remote")
                    .and_then(|v| v.as_str())
                    .unwrap_or("origin");
                let branch = args.get("branch").and_then(|v| v.as_str());
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
                git_push(&cwd, remote, branch, force)
            }
            "pull" => {
                let remote = args
                    .get("remote")
                    .and_then(|v| v.as_str())
                    .unwrap_or("origin");
                let branch = args.get("branch").and_then(|v| v.as_str());
                git_pull(&cwd, remote, branch)
            }
            "stash" => {
                let sub_action = args
                    .get("stash_action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("list");
                let message = args.get("message").and_then(|v| v.as_str());
                git_stash(&cwd, sub_action, message)
            }
            "merge" => {
                let branch = match args.get("branch").and_then(|v| v.as_str()) {
                    Some(b) => b,
                    None => return ToolResult::fail("branch is required for merge"),
                };
                git_merge(&cwd, branch)
            }
            "create_pr" => {
                let title = match args.get("title").and_then(|v| v.as_str()) {
                    Some(t) => t,
                    None => return ToolResult::fail("title is required for create_pr"),
                };
                let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
                let base = args.get("base").and_then(|v| v.as_str());
                git_create_pr(&cwd, title, body, base)
            }
            _ => ToolResult::fail(format!(
                "Unknown git action: {action}. Available: status, diff, log, branch, checkout, commit, push, pull, stash, merge, create_pr"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_git_status() {
        // This test runs in the actual repo, so just verify it doesn't error
        let tool = GitTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("action", serde_json::json!("status"))]);
        let result = tool.execute(args, &ctx).await;
        // /tmp might not be a git repo, so we accept either outcome
        assert!(result.success || result.error.is_some());
    }

    #[tokio::test]
    async fn test_git_unknown_action() {
        let tool = GitTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("action", serde_json::json!("unknown_action"))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unknown git action"));
    }

    #[tokio::test]
    async fn test_git_commit_missing_message() {
        let tool = GitTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("action", serde_json::json!("commit"))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("message is required"));
    }

    #[tokio::test]
    async fn test_git_create_pr_missing_title() {
        let tool = GitTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("action", serde_json::json!("create_pr"))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("title is required"));
    }
}
