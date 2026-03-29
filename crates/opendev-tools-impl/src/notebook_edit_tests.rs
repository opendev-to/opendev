use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn sample_notebook() -> serde_json::Value {
    serde_json::json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {},
        "cells": [
            {
                "id": "cell-001",
                "cell_type": "code",
                "metadata": {},
                "source": ["print('hello')\n"],
                "execution_count": null,
                "outputs": []
            },
            {
                "id": "cell-002",
                "cell_type": "markdown",
                "metadata": {},
                "source": ["# Title\n"]
            }
        ]
    })
}

#[test]
fn test_find_cell_by_id() {
    let nb = sample_notebook();
    let cells = nb.get("cells").unwrap().as_array().unwrap();
    assert_eq!(find_cell_index(cells, Some("cell-001"), None), Ok(0));
    assert_eq!(find_cell_index(cells, Some("cell-002"), None), Ok(1));
    assert!(find_cell_index(cells, Some("nonexistent"), None).is_err());
}

#[test]
fn test_find_cell_by_number() {
    let nb = sample_notebook();
    let cells = nb.get("cells").unwrap().as_array().unwrap();
    assert_eq!(find_cell_index(cells, None, Some(0)), Ok(0));
    assert_eq!(find_cell_index(cells, None, Some(1)), Ok(1));
    assert!(find_cell_index(cells, None, Some(5)).is_err());
    assert!(find_cell_index(cells, None, Some(-1)).is_err());
}

#[test]
fn test_find_cell_no_identifier() {
    let nb = sample_notebook();
    let cells = nb.get("cells").unwrap().as_array().unwrap();
    assert!(find_cell_index(cells, None, None).is_err());
}

#[test]
fn test_source_to_lines() {
    let lines = source_to_lines("line1\nline2\nline3");
    let arr = lines.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], "line1\n");
    assert_eq!(arr[1], "line2\n");
    assert_eq!(arr[2], "line3");
}

#[test]
fn test_source_to_lines_single() {
    let lines = source_to_lines("single line");
    let arr = lines.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0], "single line");
}

#[tokio::test]
async fn test_notebook_edit_missing_path() {
    let tool = NotebookEditTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("notebook_path is required"));
}

#[tokio::test]
async fn test_notebook_edit_file_not_found() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = NotebookEditTool;
    let ctx = ToolContext::new(&dir_path);
    let args = make_args(&[(
        "notebook_path",
        serde_json::json!(dir_path.join("nonexistent.ipynb").to_str().unwrap()),
    )]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_notebook_edit_not_ipynb() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = NotebookEditTool;
    let ctx = ToolContext::new(&dir_path);

    let path = dir_path.join("test_not_notebook.txt");
    std::fs::write(&path, "not a notebook").unwrap();

    let args = make_args(&[("notebook_path", serde_json::json!(path.to_string_lossy()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Not a Jupyter notebook"));
}

#[tokio::test]
async fn test_notebook_edit_replace() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = NotebookEditTool;
    let ctx = ToolContext::new(&dir_path);

    let path = dir_path.join("test_replace.ipynb");
    let nb = sample_notebook();
    std::fs::write(&path, serde_json::to_string_pretty(&nb).unwrap()).unwrap();

    let args = make_args(&[
        ("notebook_path", serde_json::json!(path.to_string_lossy())),
        ("cell_id", serde_json::json!("cell-001")),
        ("new_source", serde_json::json!("print('world')")),
        ("edit_mode", serde_json::json!("replace")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "Error: {:?}", result.error);
    assert!(result.output.unwrap().contains("Replaced cell"));

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let cells = saved.get("cells").unwrap().as_array().unwrap();
    let source = cells[0].get("source").unwrap().as_array().unwrap();
    assert_eq!(source[0], "print('world')");
}

#[tokio::test]
async fn test_notebook_edit_insert() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = NotebookEditTool;
    let ctx = ToolContext::new(&dir_path);

    let path = dir_path.join("test_insert.ipynb");
    let nb = sample_notebook();
    std::fs::write(&path, serde_json::to_string_pretty(&nb).unwrap()).unwrap();

    let args = make_args(&[
        ("notebook_path", serde_json::json!(path.to_string_lossy())),
        ("cell_number", serde_json::json!(1)),
        ("new_source", serde_json::json!("x = 42")),
        ("cell_type", serde_json::json!("code")),
        ("edit_mode", serde_json::json!("insert")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "Error: {:?}", result.error);
    assert!(result.output.unwrap().contains("Inserted"));

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let cells = saved.get("cells").unwrap().as_array().unwrap();
    assert_eq!(cells.len(), 3);
}

#[tokio::test]
async fn test_notebook_edit_delete() {
    let dir = tempfile::TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let tool = NotebookEditTool;
    let ctx = ToolContext::new(&dir_path);

    let path = dir_path.join("test_delete.ipynb");
    let nb = sample_notebook();
    std::fs::write(&path, serde_json::to_string_pretty(&nb).unwrap()).unwrap();

    let args = make_args(&[
        ("notebook_path", serde_json::json!(path.to_string_lossy())),
        ("cell_id", serde_json::json!("cell-002")),
        ("edit_mode", serde_json::json!("delete")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "Error: {:?}", result.error);
    assert!(result.output.unwrap().contains("Deleted"));

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let cells = saved.get("cells").unwrap().as_array().unwrap();
    assert_eq!(cells.len(), 1);
}
