//! Parameter normalization for tool invocations.
//!
//! Normalizes LLM-produced tool parameters before they reach handlers:
//! - Key normalization (camelCase -> snake_case)
//! - Whitespace stripping on string params
//! - Path resolution (relative -> absolute, ~ expansion)
//! - Workspace root guard (warn for paths outside workspace)

use std::collections::HashMap;
use std::path::Path;
use tracing::warn;

/// Parameters that contain file paths and should be resolved via `resolve_file_path`.
const FILE_PATH_PARAMS: &[&str] = &[
    "file_path",
    "notebook_path",
    "output_path",
    "plan_file_path",
    "image_path",
];

/// Parameters that contain directory paths and should be resolved via `resolve_dir_path`.
const DIR_PATH_PARAMS: &[&str] = &["path", "working_dir", "workdir"];

/// Known camelCase -> snake_case mappings from LLM errors.
fn camel_to_snake(key: &str) -> Option<&'static str> {
    match key {
        "filePath" => Some("file_path"),
        "fileName" => Some("file_name"),
        "maxResults" => Some("max_results"),
        "maxLines" => Some("max_lines"),
        "oldContent" => Some("old_content"),
        "newContent" => Some("new_content"),
        "matchAll" => Some("match_all"),
        "createDirs" => Some("create_dirs"),
        "extractText" => Some("extract_text"),
        "maxLength" => Some("max_length"),
        "includeToolCalls" => Some("include_tool_calls"),
        "sessionId" => Some("session_id"),
        "subagentType" => Some("subagent_type"),
        "detailLevel" => Some("detail_level"),
        "cellId" => Some("cell_id"),
        "cellNumber" => Some("cell_number"),
        "cellType" => Some("cell_type"),
        "editMode" => Some("edit_mode"),
        "newSource" => Some("new_source"),
        "notebookPath" => Some("notebook_path"),
        "deepCrawl" => Some("deep_crawl"),
        "crawlStrategy" => Some("crawl_strategy"),
        "maxDepth" => Some("max_depth"),
        "includeExternal" => Some("include_external"),
        "maxPages" => Some("max_pages"),
        "allowedDomains" => Some("allowed_domains"),
        "blockedDomains" => Some("blocked_domains"),
        "urlPatterns" => Some("url_patterns"),
        "symbolName" => Some("symbol_name"),
        "newName" => Some("new_name"),
        "newBody" => Some("new_body"),
        "preserveSignature" => Some("preserve_signature"),
        "includeDeclaration" => Some("include_declaration"),
        "planFilePath" => Some("plan_file_path"),
        "skillName" => Some("skill_name"),
        "taskId" => Some("task_id"),
        "runInBackground" => Some("run_in_background"),
        "toolCallId" => Some("tool_call_id"),
        "multiSelect" => Some("multi_select"),
        "activeForm" => Some("active_form"),
        "viewportWidth" => Some("viewport_width"),
        "viewportHeight" => Some("viewport_height"),
        "timeoutMs" => Some("timeout_ms"),
        "capturePdf" => Some("capture_pdf"),
        "outputPath" => Some("output_path"),
        "imagePath" => Some("image_path"),
        "imageUrl" => Some("image_url"),
        "maxTokens" => Some("max_tokens"),
        _ => None,
    }
}

/// Normalize tool parameters.
///
/// Applies in order:
/// 1. Key normalization (camelCase -> snake_case)
/// 2. Whitespace stripping on string values
/// 3. Path resolution for known path params
///
/// The original map is NOT mutated — a new map is returned.
pub fn normalize_params(
    _tool_name: &str,
    args: HashMap<String, serde_json::Value>,
    working_dir: Option<&str>,
) -> HashMap<String, serde_json::Value> {
    if args.is_empty() {
        return args;
    }

    let mut normalized = HashMap::with_capacity(args.len());

    for (key, mut value) in args {
        // 1. Key normalization
        let new_key = camel_to_snake(&key).map(String::from).unwrap_or(key);

        // 2. Whitespace stripping
        if let Some(s) = value.as_str() {
            let trimmed = s.trim();
            if trimmed.len() != s.len() {
                value = serde_json::Value::String(trimmed.to_string());
            }
        }

        // 3. Path resolution — use file resolver for file params, dir resolver for dir params
        if let Some(s) = value.as_str()
            && !s.is_empty()
        {
            let is_file = FILE_PATH_PARAMS.contains(&new_key.as_str());
            let is_dir = DIR_PATH_PARAMS.contains(&new_key.as_str());
            if (is_file || is_dir)
                && let Some(wd) = working_dir
            {
                let wd_path = Path::new(wd);
                let resolved = if is_dir {
                    crate::path::resolve_dir_path(s, wd_path)
                        .to_string_lossy()
                        .to_string()
                } else {
                    crate::path::resolve_file_path(s, wd_path)
                        .to_string_lossy()
                        .to_string()
                };
                if resolved != s {
                    warn!(
                        tool = %_tool_name,
                        param = %new_key,
                        original = %s,
                        resolved = %resolved,
                        working_dir = %wd,
                        "Path param resolved"
                    );
                }
                value = serde_json::Value::String(resolved);
            } else if is_file || is_dir {
                // No working dir: just expand home + normalize
                let resolved = resolve_path(s, working_dir);
                if resolved != s {
                    value = serde_json::Value::String(resolved);
                }
            }
        }

        normalized.insert(new_key, value);
    }

    normalized
}

/// Resolve a path string to an absolute path.
///
/// Delegates to [`crate::path::resolve_file_path`] for full resolution including
/// redundant basename detection, `~/` and `$HOME/` expansion, `./` stripping, etc.
/// Falls back to basic expansion + normalization when no working directory is available.
fn resolve_path(path_str: &str, working_dir: Option<&str>) -> String {
    if let Some(wd) = working_dir {
        let resolved = crate::path::resolve_file_path(path_str, Path::new(wd));
        let resolved_str = resolved.to_string_lossy().to_string();

        // Workspace guard: warn for paths outside workspace and home
        let in_workspace = resolved.starts_with(wd);
        let in_home = dirs::home_dir()
            .map(|h| resolved.starts_with(&h))
            .unwrap_or(false);
        if !in_workspace && !in_home {
            warn!(
                path = %resolved_str,
                workspace = %wd,
                "Path is outside workspace and user home"
            );
        }

        resolved_str
    } else {
        // No working dir: expand home + normalize, fall back to cwd
        let expanded = crate::path::expand_home(path_str);
        let path = Path::new(&expanded);

        if path.is_absolute() {
            crate::path::normalize_path(path)
                .to_string_lossy()
                .to_string()
        } else if let Ok(cwd) = std::env::current_dir() {
            crate::path::normalize_path(&cwd.join(path))
                .to_string_lossy()
                .to_string()
        } else {
            expanded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_to_snake_known() {
        assert_eq!(camel_to_snake("filePath"), Some("file_path"));
        assert_eq!(camel_to_snake("maxResults"), Some("max_results"));
        assert_eq!(camel_to_snake("sessionId"), Some("session_id"));
    }

    #[test]
    fn test_camel_to_snake_unknown() {
        assert_eq!(camel_to_snake("file_path"), None);
        assert_eq!(camel_to_snake("unknown_key"), None);
    }

    #[test]
    fn test_normalize_params_key_normalization() {
        let mut args = HashMap::new();
        args.insert("filePath".into(), serde_json::json!("/tmp/test.rs"));
        args.insert("maxResults".into(), serde_json::json!(10));

        let result = normalize_params("search", args, None);
        assert!(result.contains_key("file_path"));
        assert!(result.contains_key("max_results"));
        assert!(!result.contains_key("filePath"));
    }

    #[test]
    fn test_normalize_params_whitespace_stripping() {
        let mut args = HashMap::new();
        args.insert("query".into(), serde_json::json!("  hello world  "));

        let result = normalize_params("search", args, None);
        assert_eq!(result["query"], serde_json::json!("hello world"));
    }

    #[test]
    fn test_normalize_params_path_resolution_absolute() {
        let mut args = HashMap::new();
        args.insert("file_path".into(), serde_json::json!("/absolute/path.rs"));

        let result = normalize_params("read_file", args, Some("/workspace"));
        assert_eq!(result["file_path"], serde_json::json!("/absolute/path.rs"));
    }

    #[test]
    fn test_normalize_params_path_resolution_relative() {
        let mut args = HashMap::new();
        args.insert("file_path".into(), serde_json::json!("src/main.rs"));

        let result = normalize_params("read_file", args, Some("/workspace"));
        assert_eq!(
            result["file_path"],
            serde_json::json!("/workspace/src/main.rs")
        );
    }

    #[test]
    fn test_normalize_params_path_with_dotdot() {
        let mut args = HashMap::new();
        args.insert("file_path".into(), serde_json::json!("src/../lib.rs"));

        let result = normalize_params("read_file", args, Some("/workspace"));
        assert_eq!(result["file_path"], serde_json::json!("/workspace/lib.rs"));
    }

    #[test]
    fn test_normalize_params_tilde_expansion() {
        let mut args = HashMap::new();
        args.insert("file_path".into(), serde_json::json!("~/projects/test.rs"));

        let result = normalize_params("read_file", args, Some("/workspace"));
        let resolved = result["file_path"].as_str().unwrap();
        // Should not start with ~ anymore
        assert!(!resolved.starts_with('~'));
        assert!(resolved.ends_with("projects/test.rs"));
    }

    #[test]
    fn test_normalize_params_non_path_param_not_resolved() {
        let mut args = HashMap::new();
        args.insert("query".into(), serde_json::json!("src/main.rs"));

        let result = normalize_params("search", args, Some("/workspace"));
        // "query" is not a path param, should not be resolved
        assert_eq!(result["query"], serde_json::json!("src/main.rs"));
    }

    #[test]
    fn test_normalize_params_empty() {
        let args = HashMap::new();
        let result = normalize_params("test", args, None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_normalize_params_non_string_values_preserved() {
        let mut args = HashMap::new();
        args.insert("count".into(), serde_json::json!(42));
        args.insert("enabled".into(), serde_json::json!(true));
        args.insert("items".into(), serde_json::json!(["a", "b"]));

        let result = normalize_params("test", args, None);
        assert_eq!(result["count"], serde_json::json!(42));
        assert_eq!(result["enabled"], serde_json::json!(true));
        assert_eq!(result["items"], serde_json::json!(["a", "b"]));
    }

    #[test]
    fn test_normalize_path() {
        use std::path::PathBuf;
        assert_eq!(
            crate::path::normalize_path(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            crate::path::normalize_path(Path::new("/a/./b/c")),
            PathBuf::from("/a/b/c")
        );
        assert_eq!(
            crate::path::normalize_path(Path::new("/a/b/c")),
            PathBuf::from("/a/b/c")
        );
    }

    #[test]
    fn test_normalize_params_redundant_basename() {
        // Simulates LLM passing "myproject/main.rs" when cwd is /home/user/myproject
        // The normalizer should delegate to path::resolve_file_path which handles this.
        let mut args = HashMap::new();
        args.insert("file_path".into(), serde_json::json!("myproject/main.rs"));

        let result = normalize_params("read_file", args, Some("/home/user/myproject"));
        assert_eq!(
            result["file_path"],
            serde_json::json!("/home/user/myproject/myproject/main.rs")
        );
        // Note: without the actual filesystem, resolve_file_path can't detect the
        // redundancy (it checks .exists()). The normalizer test above confirms delegation;
        // the path module's own tests cover the filesystem-dependent redundancy detection.
    }

    #[test]
    fn test_normalize_params_path_param_resolved() {
        // The "path" param should now be resolved too
        let mut args = HashMap::new();
        args.insert("path".into(), serde_json::json!("src/lib.rs"));

        let result = normalize_params("file_search", args, Some("/workspace"));
        assert_eq!(result["path"], serde_json::json!("/workspace/src/lib.rs"));
    }

    #[test]
    fn test_normalize_params_working_dir_param_resolved() {
        let mut args = HashMap::new();
        args.insert("working_dir".into(), serde_json::json!("subdir"));

        let result = normalize_params("spawn_subagent", args, Some("/workspace"));
        assert_eq!(
            result["working_dir"],
            serde_json::json!("/workspace/subdir")
        );
    }
}
