use super::*;

#[test]
fn test_parse_frontmatter_full() {
    let content = r#"---
name: test memory
description: A test memory file
type: feedback
---

Some content here.
"#;
    let (desc, file_type) = parse_frontmatter(content);
    assert_eq!(desc, "A test memory file");
    assert_eq!(file_type, "feedback");
}

#[test]
fn test_parse_frontmatter_quoted_values() {
    let content = r#"---
name: "quoted name"
description: "quoted description"
type: "user"
---
"#;
    let (desc, file_type) = parse_frontmatter(content);
    assert_eq!(desc, "quoted description");
    assert_eq!(file_type, "user");
}

#[test]
fn test_parse_frontmatter_missing() {
    let content = "# Just a heading\n\nSome content.";
    let (desc, file_type) = parse_frontmatter(content);
    assert_eq!(desc, "");
    assert_eq!(file_type, "general");
}

#[test]
fn test_parse_frontmatter_partial() {
    let content = "---\ndescription: only desc\n---\n";
    let (desc, file_type) = parse_frontmatter(content);
    assert_eq!(desc, "only desc");
    assert_eq!(file_type, "general");
}

#[test]
fn test_format_manifest_entries() {
    let entries = vec![
        MemoryFileEntry {
            filename: "feedback_testing.md".to_string(),
            description: "Always run tests before committing".to_string(),
            file_type: "feedback".to_string(),
            modified: SystemTime::now(),
        },
        MemoryFileEntry {
            filename: "user_role.md".to_string(),
            description: "Senior Rust developer".to_string(),
            file_type: "user".to_string(),
            modified: SystemTime::UNIX_EPOCH,
        },
    ];
    let manifest = format_manifest(&entries);
    assert!(manifest.contains("[feedback] feedback_testing.md"));
    assert!(manifest.contains("Always run tests before committing"));
    assert!(manifest.contains("[user] user_role.md"));
    assert!(manifest.contains("Senior Rust developer"));
    assert!(manifest.contains("today"));
}

#[test]
fn test_format_manifest_no_description() {
    let entries = vec![MemoryFileEntry {
        filename: "notes.md".to_string(),
        description: String::new(),
        file_type: "general".to_string(),
        modified: SystemTime::now(),
    }];
    let manifest = format_manifest(&entries);
    assert!(manifest.contains("(no description)"));
}

#[test]
fn test_scan_memory_dir_empty() {
    let tmp = tempfile::TempDir::new().unwrap();
    let entries = scan_memory_dir(tmp.path());
    assert!(entries.is_empty());
}

#[test]
fn test_scan_memory_dir_with_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();

    // Create a fake memory directory structure
    let paths = opendev_config::paths::Paths::new(Some(tmp_path.clone()));
    let memory_dir = paths.project_memory_dir();
    std::fs::create_dir_all(&memory_dir).unwrap();

    // Write MEMORY.md (should be excluded)
    std::fs::write(
        memory_dir.join("MEMORY.md"),
        "# Memory Index\n- [test](test.md)\n",
    )
    .unwrap();

    // Write a memory file with frontmatter
    std::fs::write(
        memory_dir.join("feedback_testing.md"),
        "---\nname: testing feedback\ndescription: Run tests first\ntype: feedback\n---\n\nAlways run tests.\n",
    )
    .unwrap();

    // Write a plain memory file (no frontmatter)
    std::fs::write(memory_dir.join("notes.md"), "# Notes\n\nSome notes.\n").unwrap();

    let entries = scan_memory_dir(&tmp_path);
    assert_eq!(entries.len(), 2);

    // Find the frontmatter one
    let feedback = entries
        .iter()
        .find(|e| e.filename == "feedback_testing.md")
        .unwrap();
    assert_eq!(feedback.description, "Run tests first");
    assert_eq!(feedback.file_type, "feedback");

    // The plain one should have defaults
    let notes = entries.iter().find(|e| e.filename == "notes.md").unwrap();
    assert_eq!(notes.description, "");
    assert_eq!(notes.file_type, "general");
}

#[test]
fn test_read_memory_file_truncation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();

    let paths = opendev_config::paths::Paths::new(Some(tmp_path.clone()));
    let memory_dir = paths.project_memory_dir();
    std::fs::create_dir_all(&memory_dir).unwrap();

    // Write a file larger than MAX_FILE_BYTES (4096)
    let big_content = "x".repeat(8000);
    std::fs::write(memory_dir.join("big.md"), &big_content).unwrap();

    let result = read_memory_file(&tmp_path, "big.md").unwrap();
    assert!(result.len() <= MAX_FILE_BYTES);
}

#[test]
fn test_read_memory_file_nonexistent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let result = read_memory_file(tmp.path(), "nonexistent.md");
    assert!(result.is_none());
}

#[test]
fn test_dedup_surfaced_files() {
    let collector = SemanticMemoryCollector::new(15);

    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    let paths = opendev_config::paths::Paths::new(Some(tmp_path.clone()));
    let memory_dir = paths.project_memory_dir();
    std::fs::create_dir_all(&memory_dir).unwrap();

    std::fs::write(memory_dir.join("a.md"), "Content of A").unwrap();
    std::fs::write(memory_dir.join("b.md"), "Content of B").unwrap();

    // First call surfaces both
    let now = std::time::SystemTime::now();
    let selections = vec![("a.md".to_string(), now), ("b.md".to_string(), now)];
    let result1 = collector.format_selected_memories(&tmp_path, &selections);
    assert!(result1.is_some());
    let content1 = result1.unwrap();
    assert!(content1.contains("Content of A"));
    assert!(content1.contains("Content of B"));

    // Second call with same files returns None (already surfaced)
    let result2 = collector.format_selected_memories(&tmp_path, &selections);
    assert!(result2.is_none());
}

#[test]
fn test_cumulative_byte_limit() {
    let collector = SemanticMemoryCollector::new(15);

    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    let paths = opendev_config::paths::Paths::new(Some(tmp_path.clone()));
    let memory_dir = paths.project_memory_dir();
    std::fs::create_dir_all(&memory_dir).unwrap();

    // Simulate already having consumed most of the budget
    collector
        .cumulative_bytes
        .store(MAX_SESSION_BYTES - 10, Ordering::Relaxed);

    // Write a file that exceeds remaining budget
    std::fs::write(memory_dir.join("big.md"), "x".repeat(100)).unwrap();

    let selections = vec![("big.md".to_string(), std::time::SystemTime::now())];
    let result = collector.format_selected_memories(&tmp_path, &selections);
    // Should be None because 100 bytes > 10 remaining
    assert!(result.is_none());
}

#[test]
fn test_fallback_to_memory_index() {
    let tmp = tempfile::TempDir::new().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();
    let paths = opendev_config::paths::Paths::new(Some(tmp_path.clone()));
    let memory_dir = paths.project_memory_dir();
    std::fs::create_dir_all(&memory_dir).unwrap();

    // Only MEMORY.md exists, no individual files
    std::fs::write(
        memory_dir.join("MEMORY.md"),
        "# Memories\n- [note](note.md) — a note\n",
    )
    .unwrap();

    let content = SemanticMemoryCollector::load_memory_index(&tmp_path);
    assert!(content.is_some());
    assert!(content.unwrap().contains("a note"));
}
