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
