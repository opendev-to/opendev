//! Search file contents tool — delegates to ripgrep (`rg`) for fast content search.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};
use tokio::process::Command;

use crate::path_utils::resolve_dir_path;

/// Default directories to exclude from search and file listing.
/// Covers 20+ programming languages and ecosystems.
/// Ripgrep already respects `.gitignore`, but these act as a safety net
/// for repos without gitignore or for directories not tracked by git.
pub const DEFAULT_SEARCH_EXCLUDES: &[&str] = &[
    // Package/Dependency Directories
    "node_modules",
    "bower_components",
    "jspm_packages",
    "vendor",
    "Pods",
    ".bundle",
    "packages",
    ".pub-cache",
    ".pub",
    "deps",
    ".nuget",
    ".m2",
    // Virtual Environments
    ".venv",
    "venv",
    ".virtualenvs",
    ".conda",
    // Build Output Directories
    "build",
    "dist",
    "out",
    "target",
    "bin",
    "obj",
    "lib",
    "_build",
    "ebin",
    "dist-newstyle",
    ".build",
    "DerivedData",
    "CMakeFiles",
    ".cmake",
    // Framework-Specific Build
    ".next",
    ".nuxt",
    ".angular",
    ".svelte-kit",
    ".vuepress",
    ".gatsby-cache",
    ".parcel-cache",
    ".turbo",
    "dist_electron",
    // Cache Directories
    ".cache",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".hypothesis",
    ".tox",
    ".nox",
    ".eslintcache",
    ".stylelintcache",
    ".gradle",
    ".dart_tool",
    ".mix",
    ".cpcache",
    ".lsp",
    // IDE/Editor Directories
    ".idea",
    ".vscode",
    ".vscode-test",
    ".vs",
    ".metadata",
    ".settings",
    "xcuserdata",
    ".netbeans",
    // Version Control
    ".git",
    ".svn",
    ".hg",
    // Coverage/Testing Output
    "coverage",
    "htmlcov",
    ".nyc_output",
    // Language-Specific Metadata
    ".eggs",
    ".Rproj.user",
    ".julia",
    "_opam",
    ".cabal-sandbox",
    ".stack-work",
    "blib",
];

/// File glob patterns to exclude (matched by extension/suffix).
pub const DEFAULT_SEARCH_EXCLUDE_GLOBS: &[&str] = &[
    "*.min.js",
    "*.min.css",
    "*.bundle.js",
    "*.chunk.js",
    "*.map",
    "*.pyc",
    "*.pyo",
    "*.class",
    "*.o",
    "*.so",
    "*.dylib",
    "*.dll",
    "*.exe",
    "*.beam",
    "*.hi",
    "*.dyn_hi",
    "*.dyn_o",
    "*.egg-info",
];

/// Returns the path to a cached ignore file containing default exclusions.
/// The file is created once on first call and reused for all subsequent searches.
pub fn default_ignore_file() -> Option<&'static PathBuf> {
    static IGNORE_FILE: OnceLock<Option<PathBuf>> = OnceLock::new();
    IGNORE_FILE
        .get_or_init(|| {
            let mut content = String::new();
            for dir in DEFAULT_SEARCH_EXCLUDES {
                content.push_str(dir);
                content.push('/');
                content.push('\n');
            }
            for glob_pat in DEFAULT_SEARCH_EXCLUDE_GLOBS {
                content.push_str(glob_pat);
                content.push('\n');
            }
            // Write to a temp file that persists for the process lifetime
            let path = std::env::temp_dir().join("opendev-search-excludes.ignore");
            std::fs::write(&path, &content).ok()?;
            Some(path)
        })
        .as_ref()
}

/// Tool for searching file contents using ripgrep.
#[derive(Debug)]
pub struct FileSearchTool;

impl FileSearchTool {
    const TIMEOUT: Duration = Duration::from_secs(30);

    /// Build the `rg` command from the parsed arguments.
    fn build_rg_command(args: &SearchArgs, search_path: &Path) -> Command {
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

    /// Apply offset and head_limit to output lines.
    fn apply_pagination(output: &str, offset: usize, head_limit: usize) -> String {
        let lines: Vec<&str> = output.lines().collect();
        let start = offset.min(lines.len());
        let selected = &lines[start..];
        let selected = if head_limit > 0 {
            &selected[..head_limit.min(selected.len())]
        } else {
            selected
        };
        let mut result = selected.join("\n");
        if !result.is_empty() {
            result.push('\n');
        }
        result
    }
}

/// Parsed search arguments.
struct SearchArgs {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    file_type: Option<String>,
    case_insensitive: bool,
    multiline: bool,
    fixed_string: bool,
    output_mode: OutputMode,
    context: Option<u32>,
    after_context: Option<u32>,
    before_context: Option<u32>,
    line_numbers: bool,
    head_limit: usize,
    offset: usize,
    /// Search type: "text" (default, ripgrep) or "ast" (ast-grep structural search).
    search_type: SearchType,
    /// Language hint for AST mode (auto-detected from file extension if not specified).
    lang: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchType {
    Text,
    Ast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Content,
    FilesWithMatches,
    Count,
}

impl SearchArgs {
    fn from_map(args: &HashMap<String, serde_json::Value>) -> Result<Self, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "pattern is required".to_string())?
            .to_string();

        let output_mode = match args.get("output_mode").and_then(|v| v.as_str()) {
            Some("files_with_matches") => OutputMode::FilesWithMatches,
            Some("count") => OutputMode::Count,
            Some("content") | None => OutputMode::Content,
            Some(other) => {
                return Err(format!(
                    "Invalid output_mode '{other}'. Use 'content', 'files_with_matches', or 'count'"
                ));
            }
        };

        let line_numbers = args.get("-n").and_then(|v| v.as_bool()).unwrap_or(true);

        let search_type = match args.get("search_type").and_then(|v| v.as_str()) {
            Some("ast") => SearchType::Ast,
            Some("text") | None => SearchType::Text,
            Some(other) => {
                return Err(format!(
                    "Invalid search_type '{other}'. Use 'text' or 'ast'"
                ));
            }
        };

        let lang = args.get("lang").and_then(|v| v.as_str()).map(String::from);

        Ok(Self {
            pattern,
            path: args.get("path").and_then(|v| v.as_str()).map(String::from),
            glob: args.get("glob").and_then(|v| v.as_str()).map(String::from),
            file_type: args
                .get("type")
                .or_else(|| args.get("file_type"))
                .and_then(|v| v.as_str())
                .map(String::from),
            case_insensitive: args.get("-i").and_then(|v| v.as_bool()).unwrap_or(false),
            multiline: args
                .get("multiline")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            fixed_string: args
                .get("fixed_string")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            output_mode,
            context: args
                .get("context")
                .or_else(|| args.get("-C"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            after_context: args.get("-A").and_then(|v| v.as_u64()).map(|v| v as u32),
            before_context: args.get("-B").and_then(|v| v.as_u64()).map(|v| v as u32),
            line_numbers,
            head_limit: args.get("head_limit").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            offset: args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            search_type,
            lang,
        })
    }
}

#[async_trait::async_trait]
impl BaseTool for FileSearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search file contents using regex patterns (ripgrep) or AST structural patterns (ast-grep). \
         Results in files_with_matches mode are sorted by modification time (newest first). \
         Set search_type to 'ast' for syntax-aware matching with $VAR wildcards."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern for text mode, or AST pattern with $VAR wildcards for ast mode"
                },
                "search_type": {
                    "type": "string",
                    "enum": ["text", "ast"],
                    "description": "Search mode: 'text' (default) for regex via ripgrep, 'ast' for structural code search via ast-grep"
                },
                "lang": {
                    "type": "string",
                    "description": "Language hint for AST mode (e.g., 'rust', 'javascript', 'python'). Auto-detected if not specified."
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to working directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., \"*.rs\", \"*.{ts,tsx}\") — maps to rg --glob"
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

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let search_args = match SearchArgs::from_map(&args) {
            Ok(a) => a,
            Err(e) => return ToolResult::fail(e),
        };

        let search_path = search_args
            .path
            .as_deref()
            .map(|p| resolve_dir_path(p, &ctx.working_dir))
            .unwrap_or_else(|| ctx.working_dir.clone());

        if !search_path.exists() {
            return ToolResult::fail(format!("Path not found: {}", search_path.display()));
        }

        // Route based on search type
        if search_args.search_type == SearchType::Ast {
            return self.run_ast_grep(&search_args, &search_path).await;
        }

        // Try ripgrep first, fall back to built-in grep
        match self.run_rg(&search_args, &search_path).await {
            Ok(result) => result,
            Err(RgError::NotInstalled) => {
                tracing::warn!("ripgrep (rg) not found, falling back to built-in search");
                self.fallback_search(&search_args, &search_path)
            }
            Err(RgError::Timeout) => ToolResult::fail(
                "Search timed out after 30 seconds. Try a more specific pattern or path.",
            ),
            Err(RgError::Other(e)) => ToolResult::fail(format!("Search failed: {e}")),
        }
    }
}

enum RgError {
    NotInstalled,
    Timeout,
    Other(String),
}

impl FileSearchTool {
    /// Run ast-grep (sg) for structural code search.
    async fn run_ast_grep(&self, args: &SearchArgs, search_path: &Path) -> ToolResult {
        let mut cmd = Command::new("sg");
        cmd.arg("--json");
        cmd.arg("-p");
        cmd.arg(&args.pattern);

        if let Some(ref lang) = args.lang {
            cmd.arg("-l");
            cmd.arg(lang);
        }

        cmd.arg(search_path);

        let output = match tokio::time::timeout(Self::TIMEOUT, cmd.output()).await {
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
            return ToolResult::fail(format!("ast-grep error: {stderr}"));
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
                format!("{}...", &text[..200])
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

    async fn run_rg(&self, args: &SearchArgs, search_path: &Path) -> Result<ToolResult, RgError> {
        let mut cmd = Self::build_rg_command(args, search_path);

        let output = match tokio::time::timeout(Self::TIMEOUT, cmd.output()).await {
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
                let result = Self::apply_pagination(output_str, args.offset, args.head_limit);

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
                Err(RgError::Other(stderr.to_string()))
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
    fn fallback_search(&self, args: &SearchArgs, search_path: &Path) -> ToolResult {
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
                // Sort by mtime descending (newest first); files without mtime sort last
                unique_paths.sort_by(|a, b| b.1.cmp(&a.1));
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
                    let line = if m.line.len() > 500 {
                        format!("{}...", &m.line[..500])
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

        // Apply pagination
        output = Self::apply_pagination(&output, args.offset, args.head_limit);

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

/// Sort file path lines by modification time (newest first).
/// Files whose metadata cannot be read sort last.
fn sort_lines_by_mtime(lines: &str, search_path: &Path) -> String {
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

/// Get the modification time for a path, resolving relative paths against search_path.
fn get_mtime(file_path: &str, search_path: &Path) -> Option<SystemTime> {
    let p = Path::new(file_path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        search_path.join(p)
    };
    std::fs::metadata(full).and_then(|m| m.modified()).ok()
}

struct FallbackMatch {
    path: std::path::PathBuf,
    line_num: usize,
    line: String,
}

fn search_file_fallback(
    path: &Path,
    regex: &regex::Regex,
    matches: &mut Vec<FallbackMatch>,
    max: usize,
) {
    let content = match std::fs::read(path) {
        Ok(bytes) => {
            // Skip binary files
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

    // --- Unit tests for default exclusions ---

    #[test]
    fn test_build_rg_command_includes_ignore_file() {
        let args =
            SearchArgs::from_map(&make_args(&[("pattern", serde_json::json!("hello"))])).unwrap();
        let cmd = FileSearchTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();

        // Verify --ignore-file is present
        assert!(
            cmd_args.contains(&"--ignore-file".to_string()),
            "should include --ignore-file flag"
        );
    }

    #[test]
    fn test_default_ignore_file_contents() {
        let path = default_ignore_file().expect("should create ignore file");
        let content = fs::read_to_string(path).unwrap();
        assert!(
            content.contains("node_modules/"),
            "should contain node_modules/"
        );
        assert!(
            content.contains("__pycache__/"),
            "should contain __pycache__/"
        );
        assert!(content.contains(".git/"), "should contain .git/");
        assert!(content.contains("target/"), "should contain target/");
        assert!(content.contains("*.min.js"), "should contain *.min.js");
        assert!(content.contains("*.pyc"), "should contain *.pyc");
    }

    #[test]
    fn test_default_exclusion_lists_not_empty() {
        assert!(!DEFAULT_SEARCH_EXCLUDES.is_empty());
        assert!(!DEFAULT_SEARCH_EXCLUDE_GLOBS.is_empty());
        // Sanity: all directory entries are non-empty
        for entry in DEFAULT_SEARCH_EXCLUDES {
            assert!(!entry.is_empty());
        }
        // All glob patterns start with '*'
        for pat in DEFAULT_SEARCH_EXCLUDE_GLOBS {
            assert!(
                pat.starts_with('*'),
                "glob pattern should start with '*': {pat}"
            );
        }
    }

    #[tokio::test]
    async fn test_search_excludes_node_modules() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::create_dir_all(tmp.path().join("node_modules/pkg")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "fn hello() {}").unwrap();
        fs::write(
            tmp.path().join("node_modules/pkg/index.js"),
            "function hello() {}",
        )
        .unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("hello"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("main.rs"),
            "should find hello in src/main.rs"
        );
        assert!(
            !output.contains("node_modules"),
            "should not search node_modules, got: {output}"
        );
    }

    // --- Unit tests for argument parsing ---

    #[test]
    fn test_parse_args_minimal() {
        let args = make_args(&[("pattern", serde_json::json!("hello"))]);
        let parsed = SearchArgs::from_map(&args).unwrap();
        assert_eq!(parsed.pattern, "hello");
        assert_eq!(parsed.output_mode, OutputMode::Content);
        assert!(!parsed.case_insensitive);
        assert!(!parsed.multiline);
        assert!(!parsed.fixed_string);
        assert!(parsed.line_numbers);
        assert_eq!(parsed.head_limit, 0);
        assert_eq!(parsed.offset, 0);
    }

    #[test]
    fn test_parse_args_all_options() {
        let args = make_args(&[
            ("pattern", serde_json::json!("test")),
            ("path", serde_json::json!("/tmp")),
            ("glob", serde_json::json!("*.rs")),
            ("type", serde_json::json!("rust")),
            ("-i", serde_json::json!(true)),
            ("multiline", serde_json::json!(true)),
            ("fixed_string", serde_json::json!(true)),
            ("output_mode", serde_json::json!("files_with_matches")),
            ("context", serde_json::json!(3)),
            ("-A", serde_json::json!(2)),
            ("-B", serde_json::json!(1)),
            ("-n", serde_json::json!(false)),
            ("head_limit", serde_json::json!(10)),
            ("offset", serde_json::json!(5)),
        ]);
        let parsed = SearchArgs::from_map(&args).unwrap();
        assert_eq!(parsed.pattern, "test");
        assert_eq!(parsed.path.as_deref(), Some("/tmp"));
        assert_eq!(parsed.glob.as_deref(), Some("*.rs"));
        assert_eq!(parsed.file_type.as_deref(), Some("rust"));
        assert!(parsed.case_insensitive);
        assert!(parsed.multiline);
        assert!(parsed.fixed_string);
        assert_eq!(parsed.output_mode, OutputMode::FilesWithMatches);
        assert_eq!(parsed.context, Some(3));
        assert_eq!(parsed.after_context, Some(2));
        assert_eq!(parsed.before_context, Some(1));
        assert!(!parsed.line_numbers);
        assert_eq!(parsed.head_limit, 10);
        assert_eq!(parsed.offset, 5);
    }

    #[test]
    fn test_parse_args_missing_pattern() {
        let args = make_args(&[("glob", serde_json::json!("*.rs"))]);
        assert!(SearchArgs::from_map(&args).is_err());
    }

    #[test]
    fn test_parse_args_invalid_output_mode() {
        let args = make_args(&[
            ("pattern", serde_json::json!("x")),
            ("output_mode", serde_json::json!("bogus")),
        ]);
        assert!(SearchArgs::from_map(&args).is_err());
    }

    // --- Unit tests for pagination ---

    #[test]
    fn test_pagination_no_limits() {
        let input = "line1\nline2\nline3\n";
        let result = FileSearchTool::apply_pagination(input, 0, 0);
        assert_eq!(result, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_pagination_head_limit() {
        let input = "line1\nline2\nline3\nline4";
        let result = FileSearchTool::apply_pagination(input, 0, 2);
        assert_eq!(result, "line1\nline2\n");
    }

    #[test]
    fn test_pagination_offset() {
        let input = "line1\nline2\nline3\nline4";
        let result = FileSearchTool::apply_pagination(input, 2, 0);
        assert_eq!(result, "line3\nline4\n");
    }

    #[test]
    fn test_pagination_offset_and_limit() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let result = FileSearchTool::apply_pagination(input, 1, 2);
        assert_eq!(result, "line2\nline3\n");
    }

    #[test]
    fn test_pagination_offset_beyond_end() {
        let input = "line1\nline2";
        let result = FileSearchTool::apply_pagination(input, 10, 0);
        assert_eq!(result, "");
    }

    // --- Unit tests for rg command building ---

    #[test]
    fn test_build_rg_command_basic() {
        let args =
            SearchArgs::from_map(&make_args(&[("pattern", serde_json::json!("hello"))])).unwrap();
        let cmd = FileSearchTool::build_rg_command(&args, Path::new("/tmp"));
        let prog = cmd.as_std().get_program();
        assert_eq!(prog, "rg");
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--no-heading")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--color=never")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-n")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("hello")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("/tmp")));
    }

    #[test]
    fn test_build_rg_command_files_with_matches() {
        let args = SearchArgs::from_map(&make_args(&[
            ("pattern", serde_json::json!("x")),
            ("output_mode", serde_json::json!("files_with_matches")),
        ]))
        .unwrap();
        let cmd = FileSearchTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-l")));
    }

    #[test]
    fn test_build_rg_command_count() {
        let args = SearchArgs::from_map(&make_args(&[
            ("pattern", serde_json::json!("x")),
            ("output_mode", serde_json::json!("count")),
        ]))
        .unwrap();
        let cmd = FileSearchTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-c")));
    }

    #[test]
    fn test_build_rg_command_all_flags() {
        let args = SearchArgs::from_map(&make_args(&[
            ("pattern", serde_json::json!("test")),
            ("glob", serde_json::json!("*.rs")),
            ("type", serde_json::json!("rust")),
            ("-i", serde_json::json!(true)),
            ("multiline", serde_json::json!(true)),
            ("fixed_string", serde_json::json!(true)),
            ("context", serde_json::json!(3)),
            ("-A", serde_json::json!(2)),
            ("-B", serde_json::json!(1)),
        ]))
        .unwrap();
        let cmd = FileSearchTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-i")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-U")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--multiline-dotall")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-F")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--context=3")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-A=2")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-B=1")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("*.rs")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("rust")));
    }

    // --- Integration tests (require rg installed) ---

    #[tokio::test]
    async fn test_search_basic_with_rg() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("println"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("println"));
    }

    #[tokio::test]
    async fn test_search_with_glob_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn foo() {}\n").unwrap();
        fs::write(tmp.path().join("b.txt"), "fn bar() {}\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn ")),
            ("glob", serde_json::json!("*.rs")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("foo"));
        assert!(!output.contains("bar"));
    }

    #[tokio::test]
    async fn test_search_no_matches() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.txt"), "hello world\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("nonexistent"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.output.unwrap().contains("No matches"));
    }

    #[tokio::test]
    async fn test_search_files_with_matches_mode() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn foo() {}\nfn foo2() {}\n").unwrap();
        fs::write(tmp.path().join("b.rs"), "fn bar() {}\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn ")),
            ("output_mode", serde_json::json!("files_with_matches")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("a.rs"));
        assert!(output.contains("b.rs"));
        // files_with_matches should not include line content
        assert!(!output.contains("foo"));
    }

    #[tokio::test]
    async fn test_search_files_with_matches_sorted_by_mtime() {
        use std::fs::FileTimes;
        use std::time::SystemTime;

        let tmp = TempDir::new().unwrap();

        // Create files with distinct modification times (oldest first, newest last).
        let now = SystemTime::now();

        fs::write(tmp.path().join("old.rs"), "fn target() {}\n").unwrap();
        let old_time = now - Duration::from_secs(60);
        let f = fs::File::options()
            .write(true)
            .open(tmp.path().join("old.rs"))
            .unwrap();
        f.set_times(FileTimes::new().set_modified(old_time))
            .unwrap();

        fs::write(tmp.path().join("mid.rs"), "fn target() {}\n").unwrap();
        let mid_time = now - Duration::from_secs(30);
        let f = fs::File::options()
            .write(true)
            .open(tmp.path().join("mid.rs"))
            .unwrap();
        f.set_times(FileTimes::new().set_modified(mid_time))
            .unwrap();

        fs::write(tmp.path().join("new.rs"), "fn target() {}\n").unwrap();
        // new.rs keeps current mtime (newest)

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("target")),
            ("output_mode", serde_json::json!("files_with_matches")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3, "should have 3 files, got: {output}");
        // Newest first
        assert!(
            lines[0].contains("new.rs"),
            "first should be new.rs, got: {}",
            lines[0]
        );
        assert!(
            lines[1].contains("mid.rs"),
            "second should be mid.rs, got: {}",
            lines[1]
        );
        assert!(
            lines[2].contains("old.rs"),
            "third should be old.rs, got: {}",
            lines[2]
        );
    }

    #[tokio::test]
    async fn test_search_count_mode() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn ")),
            ("output_mode", serde_json::json!("count")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        // Should show count of 2
        assert!(output.contains(":2"));
    }

    #[tokio::test]
    async fn test_search_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.txt"), "Hello World\nhello world\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("HELLO")),
            ("-i", serde_json::json!(true)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("Hello"));
        assert!(output.contains("hello"));
    }

    #[tokio::test]
    async fn test_search_fixed_string() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.txt"), "a.b\na+b\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("a.b")),
            ("fixed_string", serde_json::json!(true)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        // Fixed string should match literal "a.b" but not "a+b"
        assert!(output.contains("a.b"));
        assert!(!output.contains("a+b"));
    }

    #[tokio::test]
    async fn test_search_with_context() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.txt"),
            "line1\nline2\nTARGET\nline4\nline5\n",
        )
        .unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("TARGET")),
            ("context", serde_json::json!(1)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("line2"));
        assert!(output.contains("TARGET"));
        assert!(output.contains("line4"));
    }

    #[tokio::test]
    async fn test_search_path_not_found() {
        let tool = FileSearchTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            ("pattern", serde_json::json!("x")),
            ("path", serde_json::json!("/nonexistent/path/xyz")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Path not found"));
    }

    #[tokio::test]
    async fn test_search_head_limit() {
        let tmp = TempDir::new().unwrap();
        let mut content = String::new();
        for i in 1..=20 {
            content.push_str(&format!("match line {i}\n"));
        }
        fs::write(tmp.path().join("test.txt"), &content).unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("match")),
            ("head_limit", serde_json::json!(5)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        let line_count = output.lines().count();
        assert_eq!(line_count, 5);
    }

    #[tokio::test]
    async fn test_search_with_type_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.py"), "def foo():\n    pass\n").unwrap();
        fs::write(tmp.path().join("b.rs"), "fn foo() {}\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("foo")),
            ("type", serde_json::json!("py")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("def foo"));
        assert!(!output.contains("fn foo"));
    }
}

#[cfg(test)]
mod ast_grep_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn test_parse_search_type_text() {
        let args = make_args(&[
            ("pattern", serde_json::json!("hello")),
            ("search_type", serde_json::json!("text")),
        ]);
        let parsed = SearchArgs::from_map(&args).unwrap();
        assert_eq!(parsed.search_type, SearchType::Text);
    }

    #[test]
    fn test_parse_search_type_ast() {
        let args = make_args(&[
            ("pattern", serde_json::json!("$A && $A()")),
            ("search_type", serde_json::json!("ast")),
            ("lang", serde_json::json!("javascript")),
        ]);
        let parsed = SearchArgs::from_map(&args).unwrap();
        assert_eq!(parsed.search_type, SearchType::Ast);
        assert_eq!(parsed.lang.as_deref(), Some("javascript"));
    }

    #[test]
    fn test_parse_search_type_default_is_text() {
        let args = make_args(&[("pattern", serde_json::json!("hello"))]);
        let parsed = SearchArgs::from_map(&args).unwrap();
        assert_eq!(parsed.search_type, SearchType::Text);
    }

    #[test]
    fn test_parse_search_type_invalid() {
        let args = make_args(&[
            ("pattern", serde_json::json!("hello")),
            ("search_type", serde_json::json!("invalid")),
        ]);
        assert!(SearchArgs::from_map(&args).is_err());
    }

    #[tokio::test]
    async fn test_ast_grep_basic() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn $NAME() { $$$BODY }")),
            ("search_type", serde_json::json!("ast")),
            ("lang", serde_json::json!("rust")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        // Should find the main function or report no matches
        // (ast-grep may or may not match depending on pattern specifics)
        assert!(!output.is_empty());
    }

    #[tokio::test]
    async fn test_ast_grep_no_matches() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.rs"), "fn main() {}\n").unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("class $NAME { $$$BODY }")),
            ("search_type", serde_json::json!("ast")),
            ("lang", serde_json::json!("rust")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.output.unwrap().contains("No structural matches"));
    }

    #[tokio::test]
    async fn test_ast_grep_path_not_found() {
        let tool = FileSearchTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            ("pattern", serde_json::json!("fn $NAME()")),
            ("search_type", serde_json::json!("ast")),
            ("path", serde_json::json!("/nonexistent/xyz")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_ast_grep_javascript() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.js"),
            "function hello() { return 42; }\nconst x = () => 1;\n",
        )
        .unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("function $NAME() { $$$BODY }")),
            ("search_type", serde_json::json!("ast")),
            ("lang", serde_json::json!("javascript")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_ast_grep_metadata() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.py"),
            "def hello():\n    pass\ndef world():\n    pass\n",
        )
        .unwrap();

        let tool = FileSearchTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("def $NAME(): $$$BODY")),
            ("search_type", serde_json::json!("ast")),
            ("lang", serde_json::json!("python")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        if let Some(st) = result.metadata.get("search_type") {
            assert_eq!(st, "ast");
        }
    }
}
