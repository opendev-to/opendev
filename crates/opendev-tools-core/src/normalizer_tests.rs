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

    let result = normalize_params("grep", args, None);
    assert!(result.contains_key("file_path"));
    assert!(result.contains_key("max_results"));
    assert!(!result.contains_key("filePath"));
}

#[test]
fn test_normalize_params_whitespace_stripping() {
    let mut args = HashMap::new();
    args.insert("query".into(), serde_json::json!("  hello world  "));

    let result = normalize_params("grep", args, None);
    assert_eq!(result["query"], serde_json::json!("hello world"));
}

#[cfg(unix)]
#[test]
fn test_normalize_params_path_resolution_absolute() {
    let mut args = HashMap::new();
    args.insert("file_path".into(), serde_json::json!("/absolute/path.rs"));

    let result = normalize_params("read_file", args, Some("/workspace"));
    assert_eq!(result["file_path"], serde_json::json!("/absolute/path.rs"));
}

#[cfg(unix)]
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

#[cfg(unix)]
#[test]
fn test_normalize_params_path_with_dotdot() {
    let mut args = HashMap::new();
    args.insert("file_path".into(), serde_json::json!("src/../lib.rs"));

    let result = normalize_params("read_file", args, Some("/workspace"));
    assert_eq!(result["file_path"], serde_json::json!("/workspace/lib.rs"));
}

#[cfg(unix)]
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

    let result = normalize_params("grep", args, Some("/workspace"));
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

#[cfg(unix)]
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

#[cfg(unix)]
#[test]
fn test_normalize_params_path_param_resolved() {
    // The "path" param should now be resolved too
    let mut args = HashMap::new();
    args.insert("path".into(), serde_json::json!("src/lib.rs"));

    let result = normalize_params("file_search", args, Some("/workspace"));
    assert_eq!(result["path"], serde_json::json!("/workspace/src/lib.rs"));
}

#[cfg(unix)]
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
