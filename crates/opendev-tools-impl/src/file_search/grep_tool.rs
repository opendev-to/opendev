//! GrepTool — search file contents using ripgrep.

use std::collections::HashMap;
use std::path::Path;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};
use tokio::process::Command;

use super::excludes::default_ignore_file;
use super::types::{GrepArgs, OutputMode, RgError};
use crate::dir_hints::list_available_dirs;
use crate::path_utils::resolve_dir_path;

/// Tool for searching file contents using ripgrep.
#[derive(Debug)]
pub struct GrepTool;

impl GrepTool {
    /// Build the `rg` command from the parsed arguments.
    pub(super) fn build_rg_command(args: &GrepArgs, search_path: &Path) -> Command {
        let mut cmd = Command::new("rg");

        // Always use these flags for machine-parseable output
        cmd.arg("--no-heading");
        cmd.arg("--color=never");

        // Output mode
        match args.output_mode {
            OutputMode::FilesWithMatches => {
                cmd.arg("-l");
            }
            OutputMode::Count => {
                cmd.arg("-c");
            }
            OutputMode::Content => {
                // Line numbers on by default for content mode
                if args.line_numbers {
                    cmd.arg("-n");
                }
            }
        }

        // Case insensitivity
        if args.case_insensitive {
            cmd.arg("-i");
        }

        // Multiline
        if args.multiline {
            cmd.arg("-U");
            cmd.arg("--multiline-dotall");
        }

        // Fixed string (literal, no regex)
        if args.fixed_string {
            cmd.arg("-F");
        }

        // Context lines
        if let Some(c) = args.context {
            cmd.arg(format!("--context={c}"));
        }
        if let Some(a) = args.after_context {
            cmd.arg(format!("-A={a}"));
        }
        if let Some(b) = args.before_context {
            cmd.arg(format!("-B={b}"));
        }

        // Glob filter
        if let Some(ref glob) = args.glob {
            cmd.arg("--glob");
            cmd.arg(glob);
        }

        // File type filter
        if let Some(ref file_type) = args.file_type {
            cmd.arg("--type");
            cmd.arg(file_type);
        }

        // Default exclusions via ignore file (safety net — rg already respects .gitignore).
        // Uses --ignore-file because rg's --glob override set treats negation-only
        // patterns as "exclude everything", while ignore files work correctly.
        if let Some(ignore_file) = default_ignore_file() {
            cmd.arg("--ignore-file");
            cmd.arg(ignore_file);
        }

        // Pattern and path
        cmd.arg(&args.pattern);
        cmd.arg(search_path);

        cmd
    }
}

#[async_trait::async_trait]
impl BaseTool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regex patterns via ripgrep. \
         Results in files_with_matches mode are sorted by modification time (newest first). \
         Use fixed_string=true for literal (non-regex) matching."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for (supports full regex syntax)"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to working directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., \"*.rs\", \"*.{ts,tsx}\") — maps to rg --glob"
                },
                "include": {
                    "type": "string",
                    "description": "Alias for glob — file pattern to include in the search (e.g., \"*.js\", \"*.{ts,tsx}\")"
                },
                "type": {
                    "type": "string",
                    "description": "File type to search (e.g., \"py\", \"rs\", \"js\") — maps to rg --type"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines and patterns can span lines"
                },
                "fixed_string": {
                    "type": "boolean",
                    "description": "Treat pattern as a literal string, not a regex"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: 'content' shows matching lines, 'files_with_matches' shows file paths, 'count' shows match counts"
                },
                "context": {
                    "type": "number",
                    "description": "Number of lines to show before and after each match (rg -C)"
                },
                "-A": {
                    "type": "number",
                    "description": "Number of lines to show after each match"
                },
                "-B": {
                    "type": "number",
                    "description": "Number of lines to show before each match"
                },
                "-C": {
                    "type": "number",
                    "description": "Alias for context"
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers in output (default true for content mode)"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N lines/entries"
                },
                "offset": {
                    "type": "number",
                    "description": "Skip first N lines/entries before applying head_limit"
                }
            },
            "required": ["pattern"]
        })
    }

    fn is_read_only(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn is_concurrent_safe(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Read
    }

    fn truncation_rule(&self) -> Option<opendev_tools_core::TruncationRule> {
        Some(opendev_tools_core::TruncationRule::head(10000))
    }

    fn search_hint(&self) -> Option<&str> {
        Some("search file contents with regex pattern")
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let mut grep_args = match GrepArgs::from_map(&args) {
            Ok(a) => a,
            Err(e) => return ToolResult::fail(e),
        };

        let search_path = grep_args
            .path
            .as_deref()
            .map(|p| resolve_dir_path(p, &ctx.working_dir))
            .unwrap_or_else(|| ctx.working_dir.clone());

        if !search_path.exists() {
            let available = list_available_dirs(&ctx.working_dir);
            return ToolResult::fail(format!(
                "Path not found: {}\n\nAvailable directories in working dir ({}):\n{}",
                search_path.display(),
                ctx.working_dir.display(),
                available
            ));
        }

        // If pattern is not valid regex and fixed_string wasn't explicitly set,
        // auto-enable fixed_string mode so literal patterns like "}},{"  just work.
        if !grep_args.fixed_string && regex::Regex::new(&grep_args.pattern).is_err() {
            grep_args.fixed_string = true;
        }

        // If pattern contains literal \n (newline escape), auto-enable multiline
        // so ripgrep accepts it instead of erroring.
        if !grep_args.fixed_string && grep_args.pattern.contains("\\n") {
            grep_args.multiline = true;
        }

        // Try ripgrep first, fall back to built-in grep
        match self.run_rg(&grep_args, &search_path).await {
            Ok(result) => result,
            Err(RgError::NotInstalled) => {
                tracing::warn!("ripgrep (rg) not found, falling back to built-in search");
                self.fallback_search(&grep_args, &search_path)
            }
            Err(RgError::Timeout) => ToolResult::fail(
                "Search timed out after 30 seconds. Try a more specific pattern or path.",
            ),
            Err(RgError::Other(e)) => ToolResult::fail(format!("Search failed: {e}")),
        }
    }
}
