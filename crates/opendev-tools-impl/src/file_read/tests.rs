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
async fn test_read_file_basic() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("test.txt");
    std::fs::write(&file, "line one\nline two\nline three\n").unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&dir_path);
    let args = make_args(&[("file_path", serde_json::json!(file.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("line one"));
    assert!(output.contains("line two"));
    assert!(output.contains("line three"));
}

#[tokio::test]
async fn test_read_file_with_offset_and_limit() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("lines.txt");
    let content: String = (1..=10).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&file, content).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&dir_path);
    let args = make_args(&[
        ("file_path", serde_json::json!(file.to_str().unwrap())),
        ("offset", serde_json::json!(3)),
        ("limit", serde_json::json!(2)),
    ]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("line 3"));
    assert!(output.contains("line 4"));
    assert!(!output.contains("line 5"));
}

#[tokio::test]
async fn test_read_file_not_found() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = FileReadTool;
    let ctx = ToolContext::new(&dir_path);
    let args = make_args(&[(
        "file_path",
        serde_json::json!(dir_path.join("nonexistent.txt").to_str().unwrap()),
    )]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_read_binary_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("binary.bin");
    std::fs::write(&file, &[0u8, 1, 2, 3, 0, 5]).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&dir_path);
    let args = make_args(&[("file_path", serde_json::json!(file.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Binary"));
}

#[tokio::test]
async fn test_missing_file_path() {
    let tool = FileReadTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_read_directory() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    fs::write(tmp_path.join("alpha.rs"), "").unwrap();
    fs::write(tmp_path.join("beta.txt"), "").unwrap();
    fs::create_dir(tmp_path.join("gamma")).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(tmp_path.to_str().unwrap());
    let args = make_args(&[("file_path", serde_json::json!(tmp_path.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("alpha.rs"));
    assert!(output.contains("beta.txt"));
    assert!(output.contains("gamma/"));
    // Verify sorted order: alpha < beta < gamma
    let alpha_pos = output.find("alpha.rs").unwrap();
    let beta_pos = output.find("beta.txt").unwrap();
    let gamma_pos = output.find("gamma/").unwrap();
    assert!(alpha_pos < beta_pos);
    assert!(beta_pos < gamma_pos);

    let meta = &result.metadata;
    assert_eq!(meta["total_entries"], 3);
    assert_eq!(meta["is_directory"], true);
}

#[tokio::test]
async fn test_read_directory_with_pagination() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    for name in ["aaa", "bbb", "ccc", "ddd", "eee"] {
        fs::write(tmp_path.join(name), "").unwrap();
    }

    let tool = FileReadTool;
    let ctx = ToolContext::new(tmp_path.to_str().unwrap());
    let args = make_args(&[
        ("file_path", serde_json::json!(tmp_path.to_str().unwrap())),
        ("offset", serde_json::json!(2)),
        ("limit", serde_json::json!(2)),
    ]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("bbb"));
    assert!(output.contains("ccc"));
    assert!(!output.contains("aaa"));
    assert!(!output.contains("ddd"));

    let meta = &result.metadata;
    assert_eq!(meta["total_entries"], 5);
    assert_eq!(meta["entries_shown"], 2);
}

#[tokio::test]
async fn test_read_empty_directory() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(tmp_path.to_str().unwrap());
    let args = make_args(&[("file_path", serde_json::json!(tmp_path.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("(empty directory)"));

    let meta = &result.metadata;
    assert_eq!(meta["total_entries"], 0);
}

#[tokio::test]
async fn test_file_not_found_suggestions_levenshtein() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    fs::write(tmp_path.join("file.rs"), "").unwrap();
    fs::write(tmp_path.join("file_edit.rs"), "").unwrap();
    fs::write(tmp_path.join("other.txt"), "").unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(tmp_path.to_str().unwrap());
    // "flie.rs" is a typo for "file.rs" — Levenshtein distance = 2
    let wrong_path = tmp_path.join("flie.rs");
    let args = make_args(&[("file_path", serde_json::json!(wrong_path.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("not found"));
    assert!(
        err.contains("Did you mean"),
        "Should suggest similar files, got: {err}"
    );
    assert!(
        err.contains("file.rs"),
        "Should suggest file.rs for typo flie.rs, got: {err}"
    );
}

#[tokio::test]
async fn test_file_not_found_suggestions_substring() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    fs::write(tmp_path.join("file_read.rs"), "").unwrap();
    fs::write(tmp_path.join("file_write.rs"), "").unwrap();
    fs::write(tmp_path.join("other.txt"), "").unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(tmp_path.to_str().unwrap());
    // "file" is contained in "file_read.rs" and "file_write.rs"
    let wrong_path = tmp_path.join("file");
    let args = make_args(&[("file_path", serde_json::json!(wrong_path.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("Did you mean"));
    assert!(err.contains("file_read.rs"));
    assert!(err.contains("file_write.rs"));
    assert!(!err.contains("other.txt"));
}

#[tokio::test]
async fn test_file_not_found_missing_parent_shows_dirs() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    fs::create_dir_all(tmp_path.join("crates")).unwrap();
    fs::create_dir_all(tmp_path.join("docs")).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&tmp_path);
    // Try to read a file in a non-existent "src/" directory
    let wrong_path = tmp_path.join("src/main.rs");
    let args = make_args(&[("file_path", serde_json::json!(wrong_path.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("not found"), "got: {err}");
    assert!(
        err.contains("does not exist"),
        "should note parent dir doesn't exist, got: {err}"
    );
    assert!(
        err.contains("crates/"),
        "should suggest crates/, got: {err}"
    );
}

// ---- Next offset hint ----

#[tokio::test]
async fn test_read_next_offset_hint() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("lines.txt");
    let content: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&file, content).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&dir_path);
    // Read only 5 lines from the start
    let args = make_args(&[
        ("file_path", serde_json::json!(file.to_str().unwrap())),
        ("limit", serde_json::json!(5)),
    ]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    // Should hint next offset
    assert!(
        output.contains("offset=6"),
        "Should hint offset=6, got: {output}"
    );
    assert!(output.contains("15 more lines below"));
    // Metadata should have next_offset
    assert_eq!(
        result.metadata.get("next_offset"),
        Some(&serde_json::json!(6))
    );
}

#[tokio::test]
async fn test_read_no_next_offset_at_end() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("small.txt");
    std::fs::write(&file, "line 1\nline 2\nline 3\n").unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&dir_path);
    let args = make_args(&[("file_path", serde_json::json!(file.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    // No next_offset when all lines are shown
    assert!(result.metadata.get("next_offset").is_none());
}

// ---- Output byte limit ----

#[tokio::test]
async fn test_read_large_file_byte_truncation() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();

    // Create a file with very long lines that exceed 50KB output
    let long_line = "x".repeat(500);
    let content: String = (0..200)
        .map(|_| long_line.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(tmp_path.join("big.txt"), &content).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&tmp_path);
    let args = make_args(&[(
        "file_path",
        serde_json::json!(tmp_path.join("big.txt").to_str().unwrap()),
    )]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);

    let output = result.output.unwrap();
    // Output should be capped around 50KB
    assert!(output.len() <= FileReadTool::MAX_OUTPUT_BYTES + 200); // some margin for truncation message
    if content.len() > FileReadTool::MAX_OUTPUT_BYTES {
        assert!(output.contains("truncated"));
        assert_eq!(
            result.metadata.get("truncated"),
            Some(&serde_json::json!(true))
        );
    }
}

// ---- Sensitive file warning ----

#[tokio::test]
async fn test_read_sensitive_env_file_warns() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    let env_file = tmp_path.join(".env");
    fs::write(&env_file, "SECRET_KEY=abc123\nDB_PASSWORD=hunter2\n").unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&tmp_path);
    let args = make_args(&[("file_path", serde_json::json!(env_file.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(
        output.starts_with("WARNING:"),
        "Sensitive file should have WARNING prefix, got: {}",
        &output[..output.len().min(100)]
    );
    assert!(output.contains("secrets"));
}

#[tokio::test]
async fn test_read_env_example_no_warning() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    let env_file = tmp_path.join(".env.example");
    fs::write(&env_file, "SECRET_KEY=\nDB_PASSWORD=\n").unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(&tmp_path);
    let args = make_args(&[("file_path", serde_json::json!(env_file.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;

    assert!(result.success);
    let output = result.output.unwrap();
    assert!(
        !output.starts_with("WARNING:"),
        ".env.example should NOT have warning"
    );
}
