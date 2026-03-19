//! Types and argument parsing for the grep and ast_grep tools.
//!
//! Defines search arguments, output modes, and the ripgrep error enum.

use std::collections::HashMap;

/// Parse a JSON value as u64, accepting both numbers and string representations.
/// LLMs sometimes pass `"5"` instead of `5`.
fn as_u64_lenient(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.trim().parse().ok())
}

/// Parse a JSON value as bool, accepting both booleans and string representations.
/// LLMs sometimes pass `"true"` instead of `true`.
fn as_bool_lenient(v: &serde_json::Value) -> Option<bool> {
    v.as_bool()
        .or_else(|| match v.as_str()?.trim().to_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

/// Parsed arguments for the grep (ripgrep) tool.
#[derive(Debug)]
pub(super) struct GrepArgs {
    pub pattern: String,
    pub path: Option<String>,
    pub glob: Option<String>,
    pub file_type: Option<String>,
    pub case_insensitive: bool,
    pub multiline: bool,
    pub fixed_string: bool,
    pub output_mode: OutputMode,
    pub context: Option<u32>,
    pub after_context: Option<u32>,
    pub before_context: Option<u32>,
    pub line_numbers: bool,
    pub head_limit: usize,
    pub offset: usize,
}

/// Parsed arguments for the ast_grep tool.
#[derive(Debug)]
pub(super) struct AstGrepArgs {
    pub pattern: String,
    pub path: Option<String>,
    pub lang: Option<String>,
    pub head_limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputMode {
    Content,
    FilesWithMatches,
    Count,
}

impl GrepArgs {
    pub fn from_map(args: &HashMap<String, serde_json::Value>) -> Result<Self, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "pattern is required and cannot be empty".to_string())?
            .to_string();

        let output_mode = match args
            .get("output_mode")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
        {
            Some("files_with_matches") => OutputMode::FilesWithMatches,
            Some("count") => OutputMode::Count,
            Some("content") | None => OutputMode::Content,
            Some(other) => {
                return Err(format!(
                    "Invalid output_mode '{other}'. Use 'content', 'files_with_matches', or 'count'"
                ));
            }
        };

        let line_numbers = args.get("-n").and_then(as_bool_lenient).unwrap_or(true);

        Ok(Self {
            pattern,
            path: args
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(String::from),
            glob: args
                .get("glob")
                .or_else(|| args.get("include"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(String::from),
            file_type: args
                .get("type")
                .or_else(|| args.get("file_type"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(String::from),
            case_insensitive: args.get("-i").and_then(as_bool_lenient).unwrap_or(false),
            multiline: args
                .get("multiline")
                .and_then(as_bool_lenient)
                .unwrap_or(false),
            fixed_string: args
                .get("fixed_string")
                .and_then(as_bool_lenient)
                .unwrap_or(false),
            output_mode,
            context: args
                .get("context")
                .or_else(|| args.get("-C"))
                .and_then(as_u64_lenient)
                .map(|v| v as u32),
            after_context: args.get("-A").and_then(as_u64_lenient).map(|v| v as u32),
            before_context: args.get("-B").and_then(as_u64_lenient).map(|v| v as u32),
            line_numbers,
            head_limit: args.get("head_limit").and_then(as_u64_lenient).unwrap_or(0) as usize,
            offset: args.get("offset").and_then(as_u64_lenient).unwrap_or(0) as usize,
        })
    }
}

impl AstGrepArgs {
    pub fn from_map(args: &HashMap<String, serde_json::Value>) -> Result<Self, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "pattern is required and cannot be empty".to_string())?
            .to_string();

        Ok(Self {
            pattern,
            path: args
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(String::from),
            lang: args
                .get("lang")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(String::from),
            head_limit: args.get("head_limit").and_then(as_u64_lenient).unwrap_or(0) as usize,
        })
    }
}

/// Errors that can occur when running ripgrep.
pub(super) enum RgError {
    NotInstalled,
    Timeout,
    Other(String),
}
