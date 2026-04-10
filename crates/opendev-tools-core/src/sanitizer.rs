//! Tool result sanitization — truncates large outputs before they enter LLM context.
//!
//! When a tool's output exceeds its truncation limit, the full output is saved
//! to an overflow file under `<data_dir>/tool-output/` for later retrieval via
//! `read_file` with offset/limit. Files are retained for 7 days.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Truncation strategy for a tool's output.
#[derive(Debug, Clone)]
pub enum TruncationStrategy {
    /// Keep the beginning of the text.
    Head,
    /// Keep the end of the text (most recent output).
    Tail,
    /// Keep beginning and end, cut the middle.
    HeadTail {
        /// Proportion of max_chars allocated to the head (0.0..1.0).
        head_ratio: f64,
    },
}

/// Per-tool truncation configuration.
#[derive(Debug, Clone)]
pub struct TruncationRule {
    pub max_chars: usize,
    pub strategy: TruncationStrategy,
}

impl TruncationRule {
    pub fn head(max_chars: usize) -> Self {
        Self {
            max_chars,
            strategy: TruncationStrategy::Head,
        }
    }

    pub fn tail(max_chars: usize) -> Self {
        Self {
            max_chars,
            strategy: TruncationStrategy::Tail,
        }
    }

    pub fn head_tail(max_chars: usize, head_ratio: f64) -> Self {
        Self {
            max_chars,
            strategy: TruncationStrategy::HeadTail { head_ratio },
        }
    }
}

/// Maximum characters for error messages.
const ERROR_MAX_CHARS: usize = 2000;

/// Default truncation rule for MCP tools.
fn mcp_default_rule() -> TruncationRule {
    TruncationRule::head(8000)
}

/// Built-in default rules by tool name.
fn default_rules() -> HashMap<String, TruncationRule> {
    let mut rules = HashMap::new();
    rules.insert("Bash".into(), TruncationRule::tail(8000));
    rules.insert("run_command".into(), TruncationRule::tail(8000));
    rules.insert("Read".into(), TruncationRule::head(15000));
    rules.insert("read_file".into(), TruncationRule::head(15000));
    rules.insert("Grep".into(), TruncationRule::head(10000));
    rules.insert("search".into(), TruncationRule::head(10000));
    rules.insert("Glob".into(), TruncationRule::head(10000));
    rules.insert("list_files".into(), TruncationRule::head(10000));
    rules.insert("WebFetch".into(), TruncationRule::head(12000));
    rules.insert("fetch_url".into(), TruncationRule::head(12000));
    rules.insert("WebSearch".into(), TruncationRule::head(10000));
    rules.insert("web_search".into(), TruncationRule::head(10000));
    rules.insert("browser".into(), TruncationRule::head(5000));
    rules.insert("get_session_history".into(), TruncationRule::tail(15000));
    rules.insert("memory_search".into(), TruncationRule::head(10000));
    rules
}

/// Maximum age for overflow files (7 days).
const OVERFLOW_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

/// Maximum size for overflow files (1 MB). Outputs larger than this are
/// themselves truncated before writing to disk, preventing a single runaway
/// tool call from filling the filesystem.
const MAX_OVERFLOW_BYTES: usize = 1_024 * 1_024;

/// Sanitizes tool results by applying truncation rules.
///
/// Integrates as a single pass before results enter the message history,
/// preventing context bloat from large tool outputs.
#[derive(Debug)]
pub struct ToolResultSanitizer {
    rules: HashMap<String, TruncationRule>,
    /// Directory for overflow files. If set, full output is saved when truncated.
    overflow_dir: Option<PathBuf>,
}

impl ToolResultSanitizer {
    /// Create with default rules and no overflow storage.
    pub fn new() -> Self {
        Self {
            rules: default_rules(),
            overflow_dir: None,
        }
    }

    /// Create with overflow storage enabled.
    ///
    /// When output is truncated, the full output is saved to `overflow_dir/tool_<timestamp>.txt`.
    pub fn with_overflow_dir(mut self, dir: PathBuf) -> Self {
        self.overflow_dir = Some(dir);
        self
    }

    /// Create with custom per-tool character limit overrides.
    ///
    /// Custom limits override the default max_chars but keep the default strategy.
    pub fn with_custom_limits(custom_limits: HashMap<String, usize>) -> Self {
        let mut rules = default_rules();
        for (tool_name, max_chars) in custom_limits {
            if let Some(existing) = rules.get(&tool_name) {
                rules.insert(
                    tool_name,
                    TruncationRule {
                        max_chars,
                        strategy: existing.strategy.clone(),
                    },
                );
            } else {
                rules.insert(tool_name, TruncationRule::head(max_chars));
            }
        }
        Self {
            rules,
            overflow_dir: None,
        }
    }

    /// Sanitize a tool result, truncating output if needed.
    ///
    /// Takes `success`, `output`, and `error` fields. Returns potentially
    /// truncated versions. When truncated and an overflow directory is
    /// configured, the full output is saved to disk with a retrieval hint.
    pub fn sanitize(
        &self,
        tool_name: &str,
        success: bool,
        output: Option<&str>,
        error: Option<&str>,
    ) -> SanitizedResult {
        // Truncate error messages
        if !success {
            let truncated_error = error.map(|e| {
                if e.len() > ERROR_MAX_CHARS {
                    truncate_head(e, ERROR_MAX_CHARS)
                } else {
                    e.to_string()
                }
            });
            return SanitizedResult {
                output: output.map(String::from),
                error: truncated_error,
                was_truncated: false,
                overflow_path: None,
            };
        }

        let output_str = match output {
            Some(s) if !s.is_empty() => s,
            _ => {
                return SanitizedResult {
                    output: output.map(String::from),
                    error: error.map(String::from),
                    was_truncated: false,
                    overflow_path: None,
                };
            }
        };

        let rule = match self.get_rule(tool_name) {
            Some(r) => r,
            None => {
                return SanitizedResult {
                    output: Some(output_str.to_string()),
                    error: None,
                    was_truncated: false,
                    overflow_path: None,
                };
            }
        };

        if output_str.len() <= rule.max_chars {
            return SanitizedResult {
                output: Some(output_str.to_string()),
                error: None,
                was_truncated: false,
                overflow_path: None,
            };
        }

        let original_len = output_str.len();
        let truncated = apply_strategy(output_str, rule);
        let strategy_name = match &rule.strategy {
            TruncationStrategy::Head => "head",
            TruncationStrategy::Tail => "tail",
            TruncationStrategy::HeadTail { .. } => "head_tail",
        };

        // Save full output to overflow file if configured.
        let overflow_path = self.save_overflow(tool_name, output_str);

        let mut marker = format!(
            "\n\n[truncated: showing {} of {} chars, strategy={}]",
            truncated.len(),
            original_len,
            strategy_name
        );

        // Add retrieval hint with overflow path.
        if let Some(ref path) = overflow_path {
            marker.push_str(&format!(
                "\nFull output saved to: {}\n\
                 Use read_file with offset/limit or search to access specific sections.",
                path.display()
            ));
        }

        debug!(
            tool = tool_name,
            original = original_len,
            truncated = truncated.len(),
            strategy = strategy_name,
            overflow = ?overflow_path,
            "Truncated tool result"
        );

        SanitizedResult {
            output: Some(format!("{truncated}{marker}")),
            error: None,
            was_truncated: true,
            overflow_path,
        }
    }

    /// Look up the truncation rule for a tool.
    fn get_rule(&self, tool_name: &str) -> Option<&TruncationRule> {
        // Exact match first
        if let Some(rule) = self.rules.get(tool_name) {
            return Some(rule);
        }
        // MCP tools get a default rule
        if tool_name.starts_with("mcp__") {
            // Return a reference to a leaked static for MCP default.
            // This is fine since the sanitizer lives for the program's lifetime.
            None // We handle MCP separately below
        } else {
            None
        }
    }

    /// Sanitize with MCP fallback (returns owned result).
    pub fn sanitize_with_mcp_fallback(
        &self,
        tool_name: &str,
        success: bool,
        output: Option<&str>,
        error: Option<&str>,
    ) -> SanitizedResult {
        if success && tool_name.starts_with("mcp__") && self.get_rule(tool_name).is_none() {
            // Apply MCP default rule
            if let Some(output_str) = output {
                let rule = mcp_default_rule();
                if output_str.len() > rule.max_chars {
                    let truncated = apply_strategy(output_str, &rule);
                    let overflow_path = self.save_overflow(tool_name, output_str);
                    let mut marker = format!(
                        "\n\n[truncated: showing {} of {} chars, strategy=head]",
                        truncated.len(),
                        output_str.len()
                    );
                    if let Some(ref path) = overflow_path {
                        marker.push_str(&format!(
                            "\nFull output saved to: {}\n\
                             Use read_file with offset/limit or search to access specific sections.",
                            path.display()
                        ));
                    }
                    return SanitizedResult {
                        output: Some(format!("{truncated}{marker}")),
                        error: None,
                        was_truncated: true,
                        overflow_path,
                    };
                }
            }
        }
        self.sanitize(tool_name, success, output, error)
    }

    /// Sanitize using an explicit truncation rule (from `BaseTool::truncation_rule()`).
    ///
    /// This bypasses the built-in rule map entirely, using only the provided rule.
    pub fn sanitize_with_rule(
        &self,
        tool_name: &str,
        rule: &TruncationRule,
        success: bool,
        output: Option<&str>,
        error: Option<&str>,
    ) -> SanitizedResult {
        // Truncate error messages
        if !success {
            let truncated_error = error.map(|e| {
                if e.len() > ERROR_MAX_CHARS {
                    truncate_head(e, ERROR_MAX_CHARS)
                } else {
                    e.to_string()
                }
            });
            return SanitizedResult {
                output: output.map(String::from),
                error: truncated_error,
                was_truncated: false,
                overflow_path: None,
            };
        }

        let output_str = match output {
            Some(s) if !s.is_empty() => s,
            _ => {
                return SanitizedResult {
                    output: output.map(String::from),
                    error: error.map(String::from),
                    was_truncated: false,
                    overflow_path: None,
                };
            }
        };

        if output_str.len() <= rule.max_chars {
            return SanitizedResult {
                output: Some(output_str.to_string()),
                error: None,
                was_truncated: false,
                overflow_path: None,
            };
        }

        let truncated = apply_strategy(output_str, rule);
        let strategy_name = match &rule.strategy {
            TruncationStrategy::Head => "head",
            TruncationStrategy::Tail => "tail",
            TruncationStrategy::HeadTail { .. } => "head_tail",
        };

        let overflow_path = self.save_overflow(tool_name, output_str);

        let mut marker = format!(
            "\n\n[truncated: showing {} of {} chars, strategy={}]",
            truncated.len(),
            output_str.len(),
            strategy_name
        );

        if let Some(ref path) = overflow_path {
            marker.push_str(&format!(
                "\nFull output saved to: {}\n\
                 Use read_file with offset/limit or search to access specific sections.",
                path.display()
            ));
        }

        debug!(
            tool = tool_name,
            original = output_str.len(),
            truncated = truncated.len(),
            strategy = strategy_name,
            overflow = ?overflow_path,
            "Truncated tool result via trait rule"
        );

        SanitizedResult {
            output: Some(format!("{truncated}{marker}")),
            error: None,
            was_truncated: true,
            overflow_path,
        }
    }

    /// Save full output to an overflow file. Returns the path if successful.
    fn save_overflow(&self, tool_name: &str, content: &str) -> Option<PathBuf> {
        let dir = self.overflow_dir.as_ref()?;

        // Ensure directory exists.
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!(error = %e, "Failed to create overflow directory");
            return None;
        }

        // Generate a unique filename with embedded timestamp for cleanup.
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let safe_name = tool_name.replace(['/', '\\', ':'], "_");
        let filename = format!("tool_{timestamp}_{safe_name}.txt");
        let path = dir.join(&filename);

        // Cap overflow file size to prevent disk exhaustion from huge outputs.
        let to_write = if content.len() > MAX_OVERFLOW_BYTES {
            // Use head/tail so the agent can see both the start and end of the output.
            let head_size = MAX_OVERFLOW_BYTES * 3 / 4;
            let tail_size = MAX_OVERFLOW_BYTES - head_size;
            let head: String = content.chars().take(head_size).collect();
            let tail: String = content
                .chars()
                .rev()
                .take(tail_size)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let omitted = content.len() - head_size - tail_size;
            format!("{head}\n\n[... {omitted} bytes omitted from overflow file ...]\n\n{tail}")
        } else {
            content.to_string()
        };

        match std::fs::write(&path, &to_write) {
            Ok(()) => {
                debug!(
                    path = %path.display(),
                    original_bytes = content.len(),
                    written_bytes = to_write.len(),
                    "Saved overflow output"
                );
                Some(path)
            }
            Err(e) => {
                warn!(error = %e, path = %path.display(), "Failed to save overflow output");
                None
            }
        }
    }

    /// Clean up overflow files older than 7 days.
    ///
    /// Call periodically (e.g., at startup or on a timer) to prevent
    /// unbounded disk usage from accumulated overflow files.
    pub fn cleanup_overflow(&self) {
        let Some(dir) = self.overflow_dir.as_ref() else {
            return;
        };
        cleanup_overflow_dir(dir);
    }
}

/// Remove overflow files older than [`OVERFLOW_RETENTION_SECS`] from the given directory.
pub fn cleanup_overflow_dir(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let now = std::time::SystemTime::now();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        // Extract timestamp from filename: tool_<timestamp>_<name>.txt
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if let Some(ts_str) = stem.strip_prefix("tool_")
            && let Some(ts_end) = ts_str.find('_')
            && let Ok(ts) = ts_str[..ts_end].parse::<u64>()
        {
            let file_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(ts);
            if let Ok(age) = now.duration_since(file_time)
                && age.as_secs() > OVERFLOW_RETENTION_SECS
                && let Err(e) = std::fs::remove_file(&path)
            {
                debug!(
                    path = %path.display(),
                    error = %e,
                    "Failed to remove old overflow file"
                );
            }
        }
    }
}

impl Default for ToolResultSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of sanitization.
#[derive(Debug, Clone)]
pub struct SanitizedResult {
    pub output: Option<String>,
    pub error: Option<String>,
    pub was_truncated: bool,
    /// Path to the overflow file containing the full untruncated output.
    /// Set only when `was_truncated` is true and overflow storage succeeded.
    pub overflow_path: Option<PathBuf>,
}

/// Apply a truncation strategy to text.
fn apply_strategy(text: &str, rule: &TruncationRule) -> String {
    match &rule.strategy {
        TruncationStrategy::Head => truncate_head(text, rule.max_chars),
        TruncationStrategy::Tail => truncate_tail(text, rule.max_chars),
        TruncationStrategy::HeadTail { head_ratio } => {
            truncate_head_tail(text, rule.max_chars, *head_ratio)
        }
    }
}

fn truncate_head(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn truncate_tail(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    text.chars().skip(char_count - max_chars).collect()
}

fn truncate_head_tail(text: &str, max_chars: usize, head_ratio: f64) -> String {
    let head_size = (max_chars as f64 * head_ratio) as usize;
    let tail_size = max_chars - head_size;
    let char_count = text.chars().count();

    let head: String = text.chars().take(head_size).collect();
    let tail: String = text
        .chars()
        .skip(char_count.saturating_sub(tail_size))
        .collect();
    format!("{head}\n\n... [middle truncated] ...\n\n{tail}")
}

#[cfg(test)]
#[path = "sanitizer_tests.rs"]
mod tests;
