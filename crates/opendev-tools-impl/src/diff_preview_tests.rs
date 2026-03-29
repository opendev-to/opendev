use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn test_compute_edit_script_identical() {
    let original = vec!["a", "b", "c"];
    let modified = vec!["a", "b", "c"];
    let edits = compute_edit_script(&original, &modified);
    assert!(edits.iter().all(|e| matches!(e, Edit::Keep(_, _))));
}

#[test]
fn test_compute_edit_script_addition() {
    let original = vec!["a", "c"];
    let modified = vec!["a", "b", "c"];
    let edits = compute_edit_script(&original, &modified);
    let adds: Vec<_> = edits.iter().filter(|e| matches!(e, Edit::Add(_))).collect();
    assert_eq!(adds.len(), 1);
}

#[test]
fn test_compute_edit_script_removal() {
    let original = vec!["a", "b", "c"];
    let modified = vec!["a", "c"];
    let edits = compute_edit_script(&original, &modified);
    let removes: Vec<_> = edits
        .iter()
        .filter(|e| matches!(e, Edit::Remove(_)))
        .collect();
    assert_eq!(removes.len(), 1);
}

#[test]
fn test_unified_diff_with_changes() {
    let original = vec!["line1", "line2", "line3", "line4"];
    let modified = vec!["line1", "line2 modified", "line3", "line4"];
    let diff = unified_diff(&original, &modified, "a/test.txt", "b/test.txt", 3);
    assert!(diff.contains("--- a/test.txt"));
    assert!(diff.contains("+++ b/test.txt"));
    assert!(diff.contains("-line2"));
    assert!(diff.contains("+line2 modified"));
}

#[test]
fn test_unified_diff_no_changes() {
    let lines = vec!["a", "b", "c"];
    let diff = unified_diff(&lines, &lines, "a/f.txt", "b/f.txt", 3);
    assert!(diff.is_empty());
}

#[tokio::test]
async fn test_diff_preview_missing_args() {
    let tool = DiffPreviewTool;
    let ctx = ToolContext::new("/tmp");

    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);

    let args = make_args(&[("file_path", serde_json::json!("test.txt"))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
}

#[tokio::test]
async fn test_diff_preview_with_changes() {
    let tool = DiffPreviewTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("file_path", serde_json::json!("test.rs")),
        (
            "original",
            serde_json::json!("fn main() {\n    println!(\"hello\");\n}"),
        ),
        (
            "modified",
            serde_json::json!("fn main() {\n    println!(\"world\");\n}"),
        ),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("test.rs"));
    assert!(output.contains("Changes:"));
}

#[tokio::test]
async fn test_diff_preview_no_changes() {
    let tool = DiffPreviewTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("file_path", serde_json::json!("same.txt")),
        ("original", serde_json::json!("same content")),
        ("modified", serde_json::json!("same content")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("+0") && output.contains("-0"));
}
