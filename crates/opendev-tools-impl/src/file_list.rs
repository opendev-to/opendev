//! List files tool — glob-based file listing.

use std::collections::HashMap;
use std::path::Path;

use crate::path_utils::{resolve_dir_path, validate_path_access};

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
        "list_files"
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

        if let Err(msg) = validate_path_access(&base_dir, &ctx.working_dir) {
            return ToolResult::fail(msg);
        }

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
        files.sort_by(|a, b| b.1.cmp(&a.1));

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
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_list_files_basic() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "").unwrap();
        fs::write(tmp.path().join("b.rs"), "").unwrap();
        fs::write(tmp.path().join("c.txt"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("*.rs"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("a.rs"));
        assert!(output.contains("b.rs"));
        assert!(!output.contains("c.txt"));
    }

    #[tokio::test]
    async fn test_list_files_recursive() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src/sub")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "").unwrap();
        fs::write(tmp.path().join("src/sub/lib.rs"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("**/*.rs"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("main.rs"));
        assert!(output.contains("lib.rs"));
    }

    #[tokio::test]
    async fn test_list_files_max_depth() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("a/b/c")).unwrap();
        fs::write(tmp.path().join("top.rs"), "").unwrap();
        fs::write(tmp.path().join("a/mid.rs"), "").unwrap();
        fs::write(tmp.path().join("a/b/deep.rs"), "").unwrap();
        fs::write(tmp.path().join("a/b/c/deeper.rs"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());

        // max_depth 0 = only files in base dir
        let args = make_args(&[
            ("pattern", serde_json::json!("**/*.rs")),
            ("max_depth", serde_json::json!(0)),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("top.rs"));
        assert!(!output.contains("mid.rs"));
        assert!(!output.contains("deep.rs"));

        // max_depth 1 = base dir + one level
        let args = make_args(&[
            ("pattern", serde_json::json!("**/*.rs")),
            ("max_depth", serde_json::json!(1)),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("top.rs"));
        assert!(output.contains("mid.rs"));
        assert!(!output.contains("deep.rs"));

        // No max_depth = all files
        let args = make_args(&[("pattern", serde_json::json!("**/*.rs"))]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("top.rs"));
        assert!(output.contains("mid.rs"));
        assert!(output.contains("deep.rs"));
        assert!(output.contains("deeper.rs"));
    }

    #[tokio::test]
    async fn test_list_files_excludes_node_modules() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::create_dir_all(tmp.path().join("node_modules/foo")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(
            tmp.path().join("node_modules/foo/index.js"),
            "module.exports = {}",
        )
        .unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("**/*"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("main.rs"));
        assert!(
            !output.contains("index.js"),
            "node_modules should be excluded"
        );
    }

    #[tokio::test]
    async fn test_list_files_excludes_build_dirs() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::create_dir_all(tmp.path().join("target/debug")).unwrap();
        fs::create_dir_all(tmp.path().join("__pycache__")).unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
        fs::write(tmp.path().join("target/debug/output"), "").unwrap();
        fs::write(tmp.path().join("__pycache__/mod.pyc"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("**/*"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("lib.rs"));
        assert!(!output.contains("output"), "target/ should be excluded");
        assert!(
            !output.contains("mod.pyc"),
            "__pycache__/ should be excluded"
        );
    }

    #[tokio::test]
    async fn test_list_files_excludes_minified_files() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("app.js"), "").unwrap();
        fs::write(tmp.path().join("app.min.js"), "").unwrap();
        fs::write(tmp.path().join("style.min.css"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("*"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("app.js"));
        assert!(
            !output.contains("app.min.js"),
            "*.min.js should be excluded"
        );
        assert!(
            !output.contains("style.min.css"),
            "*.min.css should be excluded"
        );
    }

    #[test]
    fn test_is_excluded_path() {
        assert!(is_excluded_path(Path::new("node_modules/foo/bar.js")));
        assert!(is_excluded_path(Path::new("src/vendor/lib.go")));
        assert!(is_excluded_path(Path::new(".git/HEAD")));
        assert!(is_excluded_path(Path::new("dist/bundle.js")));
        assert!(is_excluded_path(Path::new("app.min.js")));
        assert!(is_excluded_path(Path::new("style.min.css")));
        assert!(!is_excluded_path(Path::new("src/main.rs")));
        assert!(!is_excluded_path(Path::new("lib.rs")));
    }

    #[tokio::test]
    async fn test_list_files_custom_ignore() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("logs")).unwrap();
        fs::write(tmp.path().join("app.rs"), "").unwrap();
        fs::write(tmp.path().join("debug.log"), "").unwrap();
        fs::write(tmp.path().join("logs/app.log"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("**/*")),
            ("ignore", serde_json::json!(["*.log", "logs/"])),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("app.rs"));
        assert!(!output.contains("debug.log"), "*.log should be ignored");
        assert!(!output.contains("app.log"), "logs/ dir should be ignored");
    }

    #[tokio::test]
    async fn test_list_files_no_matches() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("*.rs"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.output.unwrap().contains("No files found"));
    }

    #[tokio::test]
    async fn test_list_files_missing_dir_shows_available() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(tmp_path.join("crates")).unwrap();
        fs::create_dir_all(tmp_path.join("docs")).unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(&tmp_path);
        let args = make_args(&[
            ("pattern", serde_json::json!("**/*.rs")),
            (
                "path",
                serde_json::json!(tmp_path.join("src").to_str().unwrap()),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("Directory not found"), "got: {err}");
        assert!(err.contains("crates/"), "should list crates/, got: {err}");
        assert!(err.contains("docs/"), "should list docs/, got: {err}");
    }

    #[tokio::test]
    async fn test_list_files_nonexistent_pattern_dir_shows_hint() {
        let tmp = TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(tmp_path.join("crates")).unwrap();
        fs::write(tmp_path.join("crates/lib.rs"), "").unwrap();

        let tool = FileListTool;
        let ctx = ToolContext::new(&tmp_path);
        let args = make_args(&[("pattern", serde_json::json!("src/**/*.rs"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(
            output.contains("does not exist"),
            "should note src/ doesn't exist, got: {output}"
        );
        assert!(
            output.contains("crates/"),
            "should suggest crates/, got: {output}"
        );
    }
}
