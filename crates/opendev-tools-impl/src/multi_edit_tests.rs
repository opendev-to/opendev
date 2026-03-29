use super::*;
use tempfile::TempDir;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_multi_edit_two_replacements() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.rs");
    std::fs::write(
        &file_path,
        "fn main() {\n    let x = 1;\n    let y = 2;\n}\n",
    )
    .unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "let x = 1;", "new_string": "let x = 10;" },
                { "old_string": "let y = 2;", "new_string": "let y = 20;" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "expected success: {:?}", result.error);

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("let x = 10;"));
    assert!(content.contains("let y = 20;"));
    assert!(!content.contains("let x = 1;"));
    assert!(!content.contains("let y = 2;"));

    // Check metadata
    assert_eq!(
        result.metadata.get("edits_applied").unwrap(),
        &serde_json::json!(2)
    );
}

#[tokio::test]
async fn test_multi_edit_sequential_dependency() {
    // Second edit depends on the result of the first
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "aaa bbb ccc").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "aaa", "new_string": "xxx" },
                { "old_string": "xxx bbb", "new_string": "yyy zzz" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "expected success: {:?}", result.error);

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "yyy zzz ccc");
}

#[tokio::test]
async fn test_multi_edit_empty_edits_fails() {
    let tool = MultiEditTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("file_path", serde_json::json!("/tmp/test.txt")),
        ("edits", serde_json::json!([])),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("must not be empty"));
}

#[tokio::test]
async fn test_multi_edit_identical_old_new_skipped() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "aaa bbb ccc").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "same", "new_string": "same" },
                { "old_string": "aaa", "new_string": "xxx" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "expected success: {:?}", result.error);

    // The no-op edit should be skipped, but the valid edit applied
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "xxx bbb ccc");
}

#[tokio::test]
async fn test_multi_edit_not_found_fails() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "hello world").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "nonexistent", "new_string": "replacement" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_multi_edit_second_edit_fails_no_write() {
    // If the second edit fails, the file should NOT be modified
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "aaa bbb ccc").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "aaa", "new_string": "xxx" },
                { "old_string": "nonexistent", "new_string": "yyy" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);

    // File should be unchanged because edits are atomic
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "aaa bbb ccc");
}

#[tokio::test]
async fn test_multi_edit_not_unique_fails() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "foo", "new_string": "qux" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("3 times"));
}

#[tokio::test]
async fn test_multi_edit_replace_all() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "foo", "new_string": "qux", "replace_all": true }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "expected success: {:?}", result.error);

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "qux bar qux baz qux");
}

#[tokio::test]
async fn test_multi_edit_diff_in_metadata() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "line1", "new_string": "LINE1" },
                { "old_string": "line3", "new_string": "LINE3" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let diff = result.metadata.get("diff").unwrap().as_str().unwrap();
    assert!(diff.contains("-line1"));
    assert!(diff.contains("+LINE1"));
    assert!(diff.contains("-line3"));
    assert!(diff.contains("+LINE3"));
}

#[tokio::test]
async fn test_multi_edit_file_not_found() {
    let tmp = TempDir::new().unwrap();
    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        (
            "file_path",
            serde_json::json!(tmp.path().join("nonexistent.txt").to_str().unwrap()),
        ),
        (
            "edits",
            serde_json::json!([
                { "old_string": "a", "new_string": "b" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_multi_edit_missing_file_path() {
    let tool = MultiEditTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[(
        "edits",
        serde_json::json!([
            { "old_string": "a", "new_string": "b" }
        ]),
    )]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("file_path is required"));
}

#[tokio::test]
async fn test_multi_edit_missing_edits() {
    let tool = MultiEditTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("file_path", serde_json::json!("/tmp/test.txt"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("edits is required"));
}

#[tokio::test]
async fn test_multi_edit_three_edits() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "alpha\nbeta\ngamma\ndelta\n").unwrap();

    let tool = MultiEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        (
            "edits",
            serde_json::json!([
                { "old_string": "alpha", "new_string": "ALPHA" },
                { "old_string": "gamma", "new_string": "GAMMA" },
                { "old_string": "delta", "new_string": "DELTA" }
            ]),
        ),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "expected success: {:?}", result.error);

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "ALPHA\nbeta\nGAMMA\nDELTA\n");

    assert_eq!(
        result.metadata.get("edits_applied").unwrap(),
        &serde_json::json!(3)
    );
}
