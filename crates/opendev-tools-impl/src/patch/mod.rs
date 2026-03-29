//! Patch tool — apply unified diff and structured (apply_patch) patches to files.

mod structured;
mod unified;

use std::collections::HashMap;
use std::path::Path;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::diagnostics_helper;

/// Tool for applying unified diff patches.
#[derive(Debug)]
pub struct PatchTool;

#[async_trait::async_trait]
impl BaseTool for PatchTool {
    fn name(&self) -> &str {
        "patch"
    }

    fn description(&self) -> &str {
        "Apply a unified diff or structured patch to files in the working directory."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Patch content in unified diff or structured (*** Begin Patch) format"
                },
                "strip": {
                    "type": "integer",
                    "description": "Number of leading path components to strip (default: 1)"
                }
            },
            "required": ["patch"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let patch_content = match args.get("patch").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::fail("patch is required"),
        };

        let strip = args.get("strip").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

        let cwd = &ctx.working_dir;

        // Detect structured patch format (*** Begin Patch)
        let mut result = if structured::is_structured_patch(patch_content) {
            structured::apply_structured_patch(patch_content, cwd)
        } else {
            // Try git apply first
            let git_result = try_git_apply(patch_content, cwd, strip).await;
            if git_result.success {
                git_result
            } else {
                // Fall back to manual patch application
                unified::apply_patch_manually(patch_content, cwd, strip)
            }
        };

        // Collect LSP diagnostics for modified files after successful patch
        if result.success
            && let Some(output) = &result.output.clone()
        {
            let modified_files = extract_modified_files(output, cwd);
            let paths: Vec<&Path> = modified_files.iter().map(|p| p.as_path()).collect();
            if !paths.is_empty()
                && let Some(diag_output) =
                    diagnostics_helper::collect_multi_file_diagnostics(ctx, &paths).await
                && let Some(ref mut out) = result.output
            {
                out.push_str(&diag_output);
            }
        }

        result
    }
}

async fn try_git_apply(patch: &str, cwd: &Path, strip: usize) -> ToolResult {
    let strip_arg = format!("-p{strip}");

    let mut child = match tokio::process::Command::new("git")
        .args(["apply", &strip_arg, "--stat", "-"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return ToolResult::fail("git not available"),
    };

    // Write patch to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(patch.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => return ToolResult::fail(format!("git apply failed: {e}")),
    };

    // Now actually apply (first was just --stat for preview)
    let mut child = match tokio::process::Command::new("git")
        .args(["apply", &strip_arg, "-"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return ToolResult::fail("git not available"),
    };

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(patch.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let apply_output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => return ToolResult::fail(format!("git apply failed: {e}")),
    };

    if apply_output.status.success() {
        let stat = String::from_utf8_lossy(&output.stdout).to_string();
        ToolResult::ok(format!("Patch applied successfully via git apply.\n{stat}"))
    } else {
        let stderr = String::from_utf8_lossy(&apply_output.stderr).to_string();
        ToolResult::fail(format!("git apply failed: {stderr}"))
    }
}

/// Extract modified file paths from patch tool output.
///
/// Parses output messages like "A path", "M path", "D path" from structured
/// patches, or file lists from manual patches.
fn extract_modified_files(output: &str, cwd: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        // Structured patch output: "A path", "M path", "R old -> new"
        if let Some(path) = trimmed
            .strip_prefix("A ")
            .or_else(|| trimmed.strip_prefix("M "))
        {
            files.push(cwd.join(path.trim()));
        } else if let Some(rest) = trimmed.strip_prefix("R ") {
            // Move: "R old -> new" — the new file is the one that exists
            if let Some((_, new_path)) = rest.split_once(" -> ") {
                files.push(cwd.join(new_path.trim()));
            }
        }
    }
    files
}

#[cfg(test)]
mod tests;
