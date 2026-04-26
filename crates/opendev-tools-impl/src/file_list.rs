//! List files tool — glob-based file listing.

use std::collections::HashMap;
use std::path::Path;

use crate::path_utils::resolve_dir_path;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::dir_hints::list_available_dirs;
use crate::file_search::{DEFAULT_SEARCH_EXCLUDE_GLOBS, DEFAULT_SEARCH_EXCLUDES};

/// Check if a path should be excluded based on default exclusion patterns.
fn is_excluded_path(path: &Path) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if DEFAULT_SEARCH_EXCLUDES.contains(&name.as_ref()) {
            return true;
        }
    }
    // Check file glob patterns (e.g., *.min.js)
    if let Some(file_name) = path.file_name() {
        let name = file_name.to_string_lossy();
        for glob_pat in DEFAULT_SEARCH_EXCLUDE_GLOBS {
            // Patterns are like "*.min.js" — check suffix after first '*'
            if let Some(suffix) = glob_pat.strip_prefix('*')
                && name.ends_with(suffix)
            {
                return true;
            }
        }
    }
    false
}

/// Tool for listing files using glob patterns.
#[derive(Debug)]
pub struct FileListTool;

impl FileListTool {
    /// Maximum number of files to return.
    const MAX_RESULTS: usize = 500;
}

#[async_trait::async_trait]
impl BaseTool for FileListTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "List files matching a glob pattern. Returns file paths sorted by modification time."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files relative to `path`. Use **/* for all files, **/*.ext for files by extension. IMPORTANT: ** alone matches directories, not files — always use **/* or **/*.ext to match files."
                },
                "path": {
                    "type": "string",
                    "description": "Base directory to search in. To list files in a subdirectory, set this to the subdirectory path instead of including it in the pattern. Defaults to working directory."
                },
                "max_depth": {
                    "type": "number",
                    "description": "Maximum directory depth to recurse into (0 = base dir only)"
                },
                "ignore": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Additional glob patterns to exclude (e.g., [\"*.log\", \"temp/\"])"
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
        Some("find files by glob pattern")
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::fail("pattern is required"),
        };

        let base_dir = args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| resolve_dir_path(p, &ctx.working_dir))
            .unwrap_or_else(|| ctx.working_dir.clone());

        let max_depth = args
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        // Parse custom ignore patterns.
        let custom_ignores: Vec<String> = args
            .get("ignore")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if !base_dir.exists() {
            let available = list_available_dirs(&ctx.working_dir);
            return ToolResult::fail(format!(
                "Directory not found: {}\n\nAvailable directories in working dir ({}):\n{}",
                base_dir.display(),
                ctx.working_dir.display(),
                available
            ));
        }

        // Build full glob pattern
        let full_pattern = base_dir.join(pattern);
        let full_pattern_str = full_pattern.to_string_lossy();

        let glob_opts = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        };

        let entries = match glob::glob_with(&full_pattern_str, glob_opts) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::fail(format!("Invalid glob pattern: {e}")),
        };

        let mut files: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();

        for entry in entries {
            match entry {
                Ok(path) => {
                    if path.is_file() {
                        // Filter out excluded directories and file patterns
                        if let Ok(rel) = path.strip_prefix(&base_dir)
                            && is_excluded_path(rel)
                        {
                            continue;
                        }
                        // Apply custom ignore patterns.
                        if !custom_ignores.is_empty()
                            && let Ok(rel) = path.strip_prefix(&base_dir)
                        {
                            let rel_str = rel.to_string_lossy();
                            let matched = custom_ignores.iter().any(|pat| {
                                // Support directory patterns (ending with /) and glob patterns.
                                if let Some(dir) = pat.strip_suffix('/') {
                                    rel_str.starts_with(dir)
                                        || rel_str.contains(&format!("/{dir}/"))
                                } else if let Ok(glob) = glob::Pattern::new(pat) {
                                    glob.matches(&rel_str)
                                } else {
                                    rel_str.contains(pat.as_str())
                                }
                            });
                            if matched {
                                continue;
                            }
                        }
                        // Apply max_depth filter: count components relative to base_dir
                        if let Some(depth) = max_depth
                            && let Ok(rel) = path.strip_prefix(&base_dir)
                        {
                            // Depth is number of parent directories (components - 1 for the file itself)
                            let rel_depth = rel.components().count().saturating_sub(1);
                            if rel_depth > depth {
                                continue;
                            }
                        }
                        let mtime = path
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        files.push((path, mtime));
                    }
                }
                Err(e) => {
                    tracing::debug!("Glob entry error: {}", e);
                }
            }
        }

        // Sort by modification time (most recent first)
        files.sort_by_key(|f| std::cmp::Reverse(f.1));

        let total = files.len();
        let truncated = total > Self::MAX_RESULTS;
        let files = &files[..total.min(Self::MAX_RESULTS)];

        if files.is_empty() {
            // Check if pattern references a non-existent directory
            let first_component = pattern.split('/').next().unwrap_or("");
            let candidate = base_dir.join(first_component);
            if !first_component.is_empty() && !first_component.contains('*') && !candidate.exists()
            {
                let available = list_available_dirs(&base_dir);
                return ToolResult::ok(format!(
                    "No files found matching '{pattern}' in {}\n\
                     Note: directory '{first_component}/' does not exist.\n\
                     Available directories:\n{available}",
                    base_dir.display()
                ));
            }
            let hint = if pattern.ends_with("**") && !pattern.ends_with("**/*") {
                "\nHint: '**' alone matches directories, not files. Try '**/*' or '**/*.ext' instead."
            } else {
                ""
            };
            return ToolResult::ok(format!(
                "No files found matching '{pattern}' in {}{}",
                base_dir.display(),
                hint
            ));
        }

        let mut output = String::new();
        for (path, _) in files {
            // Try to make path relative to base_dir
            let display = path.strip_prefix(&base_dir).unwrap_or(path).display();
            output.push_str(&format!("{display}\n"));
        }

        if truncated {
            output.push_str(&format!(
                "\n... and {} more files (showing first {})\n",
                total - Self::MAX_RESULTS,
                Self::MAX_RESULTS
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("total_files".into(), serde_json::json!(total));
        metadata.insert("truncated".into(), serde_json::json!(truncated));

        ToolResult::ok_with_metadata(output, metadata)
    }
}

#[cfg(test)]
#[path = "file_list_tests.rs"]
mod tests;
