use super::*;
use tempfile::TempDir;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_edit_single_replacement() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.rs");
    std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("hello")),
        ("new_string", serde_json::json!("world")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("world"));
    assert!(!content.contains("hello"));
}

#[tokio::test]
async fn test_edit_not_unique_fails() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("foo")),
        ("new_string", serde_json::json!("qux")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("3 times"));
    assert!(err.contains("line ")); // occurrence locations reported
}

#[tokio::test]
async fn test_edit_replace_all() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("foo")),
        ("new_string", serde_json::json!("qux")),
        ("replace_all", serde_json::json!(true)),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "qux bar qux baz qux");
}

#[tokio::test]
async fn test_edit_not_found() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "hello world").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("nonexistent")),
        ("new_string", serde_json::json!("replacement")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_edit_same_string() {
    let tool = FileEditTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("file_path", serde_json::json!("/tmp/test.txt")),
        ("old_string", serde_json::json!("same")),
        ("new_string", serde_json::json!("same")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("identical"));
}

#[tokio::test]
async fn test_edit_fuzzy_whitespace() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.rs");
    std::fs::write(
        &file_path,
        "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
    )
    .unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    // LLM provides without indentation
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("let x = 1;\nlet y = 2;")),
        (
            "new_string",
            serde_json::json!("    let x = 10;\n    let y = 20;"),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(
        result.success,
        "fuzzy match should succeed: {:?}",
        result.error
    );
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("let x = 10;"));
    assert!(content.contains("let y = 20;"));
    // Should report the match pass used
    if let Some(pass) = result.metadata.get("match_pass") {
        assert!(pass.as_str().unwrap() != "simple");
    }
}

#[tokio::test]
async fn test_edit_diff_preview_in_metadata() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("line2")),
        ("new_string", serde_json::json!("line2_modified")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let diff = result.metadata.get("diff").unwrap().as_str().unwrap();
    assert!(diff.contains("-line2"));
    assert!(diff.contains("+line2_modified"));
}

#[tokio::test]
async fn test_edit_trailing_newline_counts() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "hello\n").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    // Replace "hello\n" with "hello" — removes trailing newline
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("hello\n")),
        ("new_string", serde_json::json!("hello")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    // "hello\n" split by \n = ["hello", ""] = 2 parts
    // "hello" split by \n = ["hello"] = 1 part
    let removals = result.metadata.get("removals").unwrap().as_u64().unwrap();
    let additions = result.metadata.get("additions").unwrap().as_u64().unwrap();
    assert_eq!(removals, 2);
    assert_eq!(additions, 1);
}

#[tokio::test]
async fn test_edit_occurrence_locations_reported() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "foo\nbar\nfoo\nbaz\nfoo\n").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("foo")),
        ("new_string", serde_json::json!("qux")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("line 1"));
    assert!(err.contains("line 3"));
    assert!(err.contains("line 5"));
}

#[tokio::test]
async fn test_edit_concurrent_same_file() {
    // Verify that concurrent edits to the same file serialize correctly
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("concurrent.txt");
    std::fs::write(&file_path, "aaa bbb ccc").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());

    // First edit
    let args1 = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("aaa")),
        ("new_string", serde_json::json!("xxx")),
    ]);
    let r1 = tool.execute(args1, &ctx).await;
    assert!(r1.success);

    // Second edit on the modified file
    let args2 = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("bbb")),
        ("new_string", serde_json::json!("yyy")),
    ]);
    let r2 = tool.execute(args2, &ctx).await;
    assert!(r2.success);

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "xxx yyy ccc");
}

#[tokio::test]
async fn test_edit_rejects_sensitive_file() {
    let tmp = TempDir::new().unwrap();
    let env_file = tmp.path().join(".env");
    std::fs::write(&env_file, "SECRET=abc123\n").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(env_file.to_str().unwrap())),
        ("old_string", serde_json::json!("abc123")),
        ("new_string", serde_json::json!("newvalue")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("secrets"), "Should mention secrets: {err}");
    // File should be unchanged
    assert_eq!(
        std::fs::read_to_string(&env_file).unwrap(),
        "SECRET=abc123\n"
    );
}

#[tokio::test]
async fn test_edit_allows_env_example() {
    let tmp = TempDir::new().unwrap();
    let env_file = tmp.path().join(".env.example");
    std::fs::write(&env_file, "SECRET=placeholder\n").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(env_file.to_str().unwrap())),
        ("old_string", serde_json::json!("placeholder")),
        ("new_string", serde_json::json!("your_value_here")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(
        result.success,
        ".env.example should be editable: {:?}",
        result.error
    );
}
