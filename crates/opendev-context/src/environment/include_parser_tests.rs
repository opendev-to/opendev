use std::collections::HashSet;

use tempfile::TempDir;

use super::*;

#[test]
fn test_basic_relative_include() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::write(dir_path.join("included.md"), "Included content").unwrap();
    let content = "@./included.md\nMain content";

    let mut visited = HashSet::new();
    let (cleaned, included) =
        process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert_eq!(included.len(), 1);
    assert!(included[0].content.contains("Included content"));
    assert_eq!(included[0].source, InstructionSource::Include);
    assert!(cleaned.contains("Main content"));
    assert!(!cleaned.contains("@./included.md"));
}

#[test]
fn test_include_without_dot_slash() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::write(dir_path.join("rules.md"), "Rule content").unwrap();
    let content = "@rules.md\nMain";

    let mut visited = HashSet::new();
    let (_, included) = process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert_eq!(included.len(), 1);
    assert!(included[0].content.contains("Rule content"));
}

#[test]
fn test_include_absolute_path() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let file_path = dir_path.join("absolute.md");
    std::fs::write(&file_path, "Absolute content").unwrap();
    let content = format!("@{}\nMain", file_path.display());

    let mut visited = HashSet::new();
    let (_, included) = process_includes(&content, &dir_path, &dir_path, 0, &mut visited, None);

    assert_eq!(included.len(), 1);
    assert!(included[0].content.contains("Absolute content"));
}

#[test]
fn test_max_depth_prevents_infinite_recursion() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    // Create file that includes itself (shouldn't recurse past MAX_INCLUDE_DEPTH)
    std::fs::write(dir_path.join("self.md"), "@./self.md\nContent").unwrap();

    let mut visited = HashSet::new();
    let (_, included) = process_includes(
        "@./self.md\nMain",
        &dir_path,
        &dir_path,
        0,
        &mut visited,
        None,
    );

    // Should include the file once (circular ref prevention)
    assert_eq!(included.len(), 1);
}

#[test]
fn test_circular_reference_detected() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::write(dir_path.join("a.md"), "@./b.md\nFile A").unwrap();
    std::fs::write(dir_path.join("b.md"), "@./a.md\nFile B").unwrap();

    let mut visited = HashSet::new();
    let (_, included) =
        process_includes("@./a.md\nMain", &dir_path, &dir_path, 0, &mut visited, None);

    // a.md includes b.md, but b.md trying to include a.md should be skipped
    assert_eq!(included.len(), 2); // b.md and a.md
}

#[test]
fn test_binary_file_skipped() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::write(dir_path.join("image.png"), "fake png").unwrap();
    let content = "@./image.png\nMain content";

    let mut visited = HashSet::new();
    let (_, included) = process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert!(included.is_empty());
}

#[test]
fn test_include_in_code_block_ignored() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::write(dir_path.join("should_not_include.md"), "Oops").unwrap();
    let content = "Before\n```\n@./should_not_include.md\n```\nAfter";

    let mut visited = HashSet::new();
    let (cleaned, included) =
        process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert!(included.is_empty());
    assert!(cleaned.contains("@./should_not_include.md"));
}

#[test]
fn test_multiple_includes() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::write(dir_path.join("first.md"), "First").unwrap();
    std::fs::write(dir_path.join("second.md"), "Second").unwrap();
    let content = "@./first.md\n@./second.md\nMain";

    let mut visited = HashSet::new();
    let (_, included) = process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert_eq!(included.len(), 2);
}

#[test]
fn test_nonexistent_include_skipped() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let content = "@./does_not_exist.md\nMain content";

    let mut visited = HashSet::new();
    let (cleaned, included) =
        process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert!(included.is_empty());
    assert!(cleaned.contains("Main content"));
}

#[test]
fn test_include_with_fragment_stripped() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::write(dir_path.join("doc.md"), "Doc content").unwrap();
    let content = "@./doc.md#section-1\nMain";

    let mut visited = HashSet::new();
    let (_, included) = process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert_eq!(included.len(), 1);
    assert!(included[0].content.contains("Doc content"));
}

#[test]
fn test_double_at_not_treated_as_include() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let content = "@@not-an-include\nMain";

    let mut visited = HashSet::new();
    let (cleaned, included) =
        process_includes(content, &dir_path, &dir_path, 0, &mut visited, None);

    assert!(included.is_empty());
    assert!(cleaned.contains("@@not-an-include"));
}

#[test]
fn test_included_from_field_set() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let parent = dir_path.join("parent.md");
    std::fs::write(dir_path.join("child.md"), "Child").unwrap();
    std::fs::write(&parent, "@./child.md\nParent content").unwrap();

    let mut visited = HashSet::new();
    let (_, included) = process_includes(
        "@./child.md\nMain",
        &dir_path,
        &dir_path,
        0,
        &mut visited,
        Some(&parent),
    );

    assert_eq!(included.len(), 1);
    assert_eq!(included[0].included_from.as_ref().unwrap(), &parent);
}
