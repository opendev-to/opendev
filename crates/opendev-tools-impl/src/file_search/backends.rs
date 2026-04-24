//! Search backend implementations: ripgrep, ast-grep, and fallback regex search.
//!
//! Contains the actual search execution logic, result sorting, and the
//! built-in fallback for environments without ripgrep.

use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

use opendev_tools_core::ToolResult;
use tokio::process::Command;

use super::types::{AstGrepArgs, GrepArgs, OutputMode, RgError};
use super::{AstGrepTool, GrepTool, SEARCH_TIMEOUT, apply_pagination};

// ---------------------------------------------------------------------------
// AST-grep search
// ---------------------------------------------------------------------------

impl AstGrepTool {
    /// Run ast-grep (sg) for structural code search.
    pub(super) async fn run_ast_grep(&self, args: &AstGrepArgs, search_path: &Path) -> ToolResult {
        let mut cmd = Command::new("sg");
        cmd.arg("--json");
        cmd.arg("-p");
        cmd.arg(&args.pattern);

        if let Some(ref lang) = args.lang {
            cmd.arg("-l");
            cmd.arg(lang);
        }

        cmd.arg(search_path);

        let output = match tokio::time::timeout(SEARCH_TIMEOUT, cmd.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return ToolResult::fail(
                        "ast-grep (sg) not installed. Install: brew install ast-grep",
                    );
                }
                return ToolResult::fail(format!("ast-grep failed: {e}"));
            }
            Err(_) => {
                return ToolResult::fail(
                    "AST search timed out after 30 seconds. Try a more specific path.",
                );
            }
        };

        if !output.status.success() && output.stdout.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.trim().is_empty() {
                return ToolResult::ok("No structural matches found");
            }
            let stderr_str = stderr.trim();
            let mut msg = format!("ast-grep error: {stderr_str}");

            if stderr_str.contains("Multiple AST nodes") {
                msg.push_str(
                    "\n\nHint: Pattern parses as multiple separate AST nodes. \
                     Use a single expression or statement pattern.",
                );
            } else if stderr_str.contains("parse") || stderr_str.contains("Cannot parse") {
                msg.push_str(
                    "\n\nHint: Pattern must be valid standalone syntax in the target language. \
                     Common mistakes:\n\
                     - Class method definitions like `name(): void {}` are not standalone \
                     — use `function name(): void {}` or search by call: `$OBJ.name($$$ARGS)`\n\
                     - Rust functions need visibility: `pub fn $NAME($$$ARGS)` not `fn $NAME($$$ARGS)`",
                );
            }

            return ToolResult::fail(msg);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return ToolResult::ok("No structural matches found");
        }

        // Parse JSON output from ast-grep
        let data: Vec<serde_json::Value> = match serde_json::from_str(&stdout) {
            Ok(d) => d,
            Err(_) => return ToolResult::ok("No structural matches found"),
        };

        if data.is_empty() {
            return ToolResult::ok("No structural matches found");
        }

        let max_results = if args.head_limit > 0 {
            args.head_limit
        } else {
            50
        };

        let mut lines = Vec::new();
        let mut count = 0usize;

        for item in &data {
            if count >= max_results {
                break;
            }

            let file = item.get("file").and_then(|v| v.as_str()).unwrap_or("");

            // Normalize to relative path
            let rel_path = if let Ok(rel) = Path::new(file).strip_prefix(search_path) {
                rel.display().to_string()
            } else if let Ok(stripped) = Path::new(file).canonicalize() {
                if let Ok(sp) = search_path.canonicalize() {
                    stripped
                        .strip_prefix(&sp)
                        .map(|r| r.display().to_string())
                        .unwrap_or_else(|_| file.to_string())
                } else {
                    file.to_string()
                }
            } else {
                file.to_string()
            };

            let line_num = item
                .get("range")
                .and_then(|r| r.get("start"))
                .and_then(|s| s.get("line"))
                .and_then(|l| l.as_u64())
                .unwrap_or(0);

            let text = item
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();

            // Truncate very long matches
            let display_text = if text.len() > 200 {
                format!("{}...", &text[..text.floor_char_boundary(200)])
            } else {
                text.to_string()
            };

            lines.push(format!("{rel_path}:{line_num} - {display_text}"));
            count += 1;
        }

        if lines.is_empty() {
            return ToolResult::ok("No structural matches found");
        }

        let total = data.len();
        let mut result = lines.join("\n");
        if total > count {
            result.push_str(&format!("\n\n... ({count} of {total} matches shown)"));
        }

        let mut metadata = HashMap::new();
        metadata.insert("match_count".into(), serde_json::json!(total));
        metadata.insert("search_type".into(), serde_json::json!("ast"));

        ToolResult::ok_with_metadata(result, metadata)
    }
}

// ---------------------------------------------------------------------------
// Ripgrep search
// ---------------------------------------------------------------------------

impl GrepTool {
    pub(super) async fn run_rg(
        &self,
        args: &GrepArgs,
        search_path: &Path,
    ) -> Result<ToolResult, RgError> {
        let mut cmd = Self::build_rg_command(args, search_path);

        let output = match tokio::time::timeout(SEARCH_TIMEOUT, cmd.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(RgError::NotInstalled);
                }
                return Err(RgError::Other(e.to_string()));
            }
            Err(_) => return Err(RgError::Timeout),
        };

        // rg exit codes: 0 = matches found, 1 = no matches, 2 = error
        match output.status.code() {
            Some(0) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Sort by mtime (newest first) for files_with_matches mode
                let sorted;
                let output_str = if args.output_mode == OutputMode::FilesWithMatches {
                    sorted = sort_lines_by_mtime(&stdout, search_path);
                    sorted.as_str()
                } else {
                    &stdout
                };
                let result = apply_pagination(output_str, args.offset, args.head_limit);

                if result.trim().is_empty() {
                    return Ok(ToolResult::ok(format!(
                        "No matches found for '{}' in {} (after offset/limit)",
                        args.pattern,
                        search_path.display()
                    )));
                }

                let line_count = result.lines().count();
                let mut metadata = HashMap::new();
                metadata.insert("match_count".into(), serde_json::json!(line_count));

                Ok(ToolResult::ok_with_metadata(result, metadata))
            }
            Some(1) => Ok(ToolResult::ok(format!(
                "No matches found for '{}' in {}",
                args.pattern,
                search_path.display()
            ))),
            Some(2) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("unrecognized file type") {
                    Err(RgError::Other(format!(
                        "Invalid file type '{}'. Run `rg --type-list` to see valid types. Common: py, rs, js, ts, go, java, c, cpp",
                        args.file_type.as_deref().unwrap_or("unknown")
                    )))
                } else {
                    Err(RgError::Other(stderr.to_string()))
                }
            }
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(RgError::Other(format!(
                    "rg exited with unexpected status: {}",
                    stderr
                )))
            }
        }
    }

    /// Fallback: built-in regex search when rg is not installed.
    pub(super) fn fallback_search(&self, args: &GrepArgs, search_path: &Path) -> ToolResult {
        let regex = match regex::Regex::new(&args.pattern) {
            Ok(r) => r,
            Err(e) => return ToolResult::fail(format!("Invalid regex pattern: {e}")),
        };

        let mut matches = Vec::new();
        const MAX_RESULTS: usize = 200;

        if search_path.is_file() {
            search_file_fallback(search_path, &regex, &mut matches, MAX_RESULTS);
        } else {
            let glob_pattern = args.glob.as_deref().unwrap_or("**/*");
            let full_pattern = search_path.join(glob_pattern);

            let entries = match glob::glob(&full_pattern.to_string_lossy()) {
                Ok(e) => e,
                Err(e) => return ToolResult::fail(format!("Invalid glob: {e}")),
            };

            for entry in entries {
                if matches.len() >= MAX_RESULTS {
                    break;
                }
                if let Ok(path) = entry
                    && path.is_file()
                {
                    search_file_fallback(&path, &regex, &mut matches, MAX_RESULTS);
                }
            }
        }

        if matches.is_empty() {
            return ToolResult::ok(format!(
                "No matches found for '{}' in {}",
                args.pattern,
                search_path.display()
            ));
        }

        let total = matches.len();
        let truncated = total >= MAX_RESULTS;

        let mut output = String::new();
        match args.output_mode {
            OutputMode::FilesWithMatches => {
                let mut seen = std::collections::HashSet::new();
                let mut unique_paths: Vec<(String, Option<SystemTime>)> = Vec::new();
                for m in &matches {
                    let rel = m.path.strip_prefix(search_path).unwrap_or(&m.path);
                    let key = rel.display().to_string();
                    if seen.insert(key.clone()) {
                        let mtime = std::fs::metadata(&m.path).and_then(|md| md.modified()).ok();
                        unique_paths.push((key, mtime));
                    }
                }
                unique_paths.sort_by_key(|b| std::cmp::Reverse(b.1));
                for (key, _) in &unique_paths {
                    output.push_str(key);
                    output.push('\n');
                }
            }
            OutputMode::Count => {
                let mut counts: HashMap<String, usize> = HashMap::new();
                for m in &matches {
                    let rel = m.path.strip_prefix(search_path).unwrap_or(&m.path);
                    *counts.entry(rel.display().to_string()).or_default() += 1;
                }
                for (path, count) in &counts {
                    output.push_str(&format!("{path}:{count}\n"));
                }
            }
            OutputMode::Content => {
                for m in &matches {
                    let rel = m.path.strip_prefix(search_path).unwrap_or(&m.path);
                    let line = if m.line.len() > 2000 {
                        format!("{}...", &m.line[..m.line.floor_char_boundary(2000)])
                    } else {
                        m.line.clone()
                    };
                    if args.line_numbers {
                        output.push_str(&format!("{}:{}: {}\n", rel.display(), m.line_num, line));
                    } else {
                        output.push_str(&format!("{}:{}\n", rel.display(), line));
                    }
                }
            }
        }

        output = apply_pagination(&output, args.offset, args.head_limit);

        if truncated {
            output.push_str(&format!("\n(showing first {MAX_RESULTS} matches)\n"));
        }

        let mut metadata = HashMap::new();
        metadata.insert("match_count".into(), serde_json::json!(total));
        metadata.insert("truncated".into(), serde_json::json!(truncated));
        metadata.insert("fallback".into(), serde_json::json!(true));

        ToolResult::ok_with_metadata(output, metadata)
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Sort file path lines by modification time (newest first).
pub(super) fn sort_lines_by_mtime(lines: &str, search_path: &Path) -> String {
    let mut paths: Vec<&str> = lines.lines().filter(|l| !l.is_empty()).collect();
    paths.sort_by(|a, b| {
        let mtime_a = get_mtime(a, search_path);
        let mtime_b = get_mtime(b, search_path);
        mtime_b.cmp(&mtime_a)
    });
    let mut result = paths.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}

fn get_mtime(file_path: &str, search_path: &Path) -> Option<SystemTime> {
    let p = Path::new(file_path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        search_path.join(p)
    };
    std::fs::metadata(full).and_then(|m| m.modified()).ok()
}

pub(super) struct FallbackMatch {
    pub path: std::path::PathBuf,
    pub line_num: usize,
    pub line: String,
}

pub(super) fn search_file_fallback(
    path: &Path,
    regex: &regex::Regex,
    matches: &mut Vec<FallbackMatch>,
    max: usize,
) {
    let content = match std::fs::read(path) {
        Ok(bytes) => {
            if bytes.iter().take(8192).any(|&b| b == 0) {
                return;
            }
            String::from_utf8_lossy(&bytes).to_string()
        }
        Err(_) => return,
    };

    for (i, line) in content.lines().enumerate() {
        if matches.len() >= max {
            return;
        }
        if regex.is_match(line) {
            matches.push(FallbackMatch {
                path: path.to_path_buf(),
                line_num: i + 1,
                line: line.to_string(),
            });
        }
    }
}
