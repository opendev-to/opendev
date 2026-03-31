use super::*;
use tempfile::TempDir;

#[test]
fn test_memory_write_and_read() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();

    let result = memory_write(&dir, "test.md", "hello world");
    assert!(result.success);

    let result = memory_read(&dir, "test.md");
    assert!(result.success);
    assert_eq!(result.output.unwrap(), "hello world");
}

#[test]
fn test_memory_read_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let result = memory_read(tmp.path(), "nope.md");
    assert!(!result.success);
}

#[test]
fn test_memory_path_traversal_blocked() {
    let tmp = TempDir::new().unwrap();
    let result = memory_read(tmp.path(), "../etc/passwd");
    assert!(!result.success);
    assert!(result.error.unwrap().contains("path traversal"));
}

#[test]
fn test_memory_search() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("notes.md"),
        "Rust is a systems language\nPython is dynamic",
    )
    .unwrap();
    std::fs::write(tmp.path().join("other.md"), "unrelated content").unwrap();

    let result = memory_search(tmp.path(), "rust systems");
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(out.contains("notes.md"));
    assert!(!out.contains("other.md"));
}

#[test]
fn test_memory_list() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.md"), "aaa").unwrap();
    std::fs::write(tmp.path().join("b.md"), "bbb").unwrap();

    let result = memory_list(tmp.path());
    assert!(result.success);
    let out = result.output.unwrap();
    assert!(out.contains("a.md"));
    assert!(out.contains("b.md"));
}

#[test]
fn test_resolve_memory_dir_project() {
    let dir = resolve_memory_dir("project", Path::new("/tmp/test-project"));
    assert!(dir.is_some());
    let path = dir.unwrap();
    assert!(path.to_string_lossy().contains("projects"));
    assert!(path.to_string_lossy().contains("memory"));
}

#[test]
fn test_resolve_memory_dir_global() {
    let dir = resolve_memory_dir("global", Path::new("/tmp/test-project"));
    assert!(dir.is_some());
    let path = dir.unwrap();
    assert!(path.to_string_lossy().ends_with(".opendev/memory"));
}

#[test]
fn test_update_memory_index_generates_index() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("patterns.md"), "Use snake_case everywhere").unwrap();
    std::fs::write(
        tmp.path().join("decisions.md"),
        "# Decisions\nWe chose Rust",
    )
    .unwrap();

    update_memory_index(tmp.path()).unwrap();

    let index = std::fs::read_to_string(tmp.path().join("MEMORY.md")).unwrap();
    assert!(index.starts_with("# Memory Index"));
    assert!(index.contains("[decisions.md]"));
    assert!(index.contains("[patterns.md]"));
    assert!(index.contains("Use snake_case everywhere"));
}

#[test]
fn test_update_memory_index_skips_memory_md() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("MEMORY.md"), "old index").unwrap();
    std::fs::write(tmp.path().join("notes.md"), "some notes").unwrap();

    update_memory_index(tmp.path()).unwrap();

    let index = std::fs::read_to_string(tmp.path().join("MEMORY.md")).unwrap();
    assert!(index.contains("[notes.md]"));
    assert!(!index.contains("[MEMORY.md]"));
}

#[test]
fn test_update_memory_index_empty_dir() {
    let tmp = TempDir::new().unwrap();
    update_memory_index(tmp.path()).unwrap();

    let index = std::fs::read_to_string(tmp.path().join("MEMORY.md")).unwrap();
    assert_eq!(index, "# Memory Index");
}

#[test]
fn test_extract_description_frontmatter() {
    let content = "---\nname: test\ndescription: My description\n---\n# Content";
    assert_eq!(extract_description(content), "My description");
}

#[test]
fn test_extract_description_first_line() {
    let content = "# Heading\nFirst real content line\nSecond line";
    assert_eq!(extract_description(content), "First real content line");
}

#[test]
fn test_extract_description_empty() {
    assert_eq!(extract_description(""), String::new());
    assert_eq!(extract_description("# Only heading"), String::new());
}

#[test]
fn test_update_memory_index_skips_non_md_files() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("notes.md"), "some notes").unwrap();
    std::fs::write(tmp.path().join("data.json"), r#"{"key": "value"}"#).unwrap();

    update_memory_index(tmp.path()).unwrap();

    let index = std::fs::read_to_string(tmp.path().join("MEMORY.md")).unwrap();
    assert!(index.contains("[notes.md]"));
    assert!(!index.contains("data.json"));
}
