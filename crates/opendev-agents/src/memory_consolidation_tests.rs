use super::*;
use tempfile::TempDir;

#[test]
fn test_parse_type_and_body_with_frontmatter() {
    let content = "---\ntype: session\ndescription: test\n---\n\nBody content here";
    let (ft, body) = parse_type_and_body(content);
    assert_eq!(ft, "session");
    assert_eq!(body, "Body content here");
}

#[test]
fn test_parse_type_and_body_without_frontmatter() {
    let content = "Just some plain text\nwith multiple lines";
    let (ft, body) = parse_type_and_body(content);
    assert_eq!(ft, "general");
    assert_eq!(body, content.trim());
}

#[test]
fn test_count_session_files_empty() {
    let dir = TempDir::new().unwrap();
    assert_eq!(count_session_files(dir.path()), 0);
}

#[test]
fn test_count_session_files_mixed() {
    let dir = TempDir::new().unwrap();

    // Session file (by name prefix)
    std::fs::write(
        dir.path().join("session-2026-01-01-abc.md"),
        "---\ntype: session\n---\nnotes",
    )
    .unwrap();

    // Non-session file
    std::fs::write(
        dir.path().join("patterns.md"),
        "---\ntype: project\n---\npatterns",
    )
    .unwrap();

    // Session file (by type, not name prefix)
    std::fs::write(
        dir.path().join("notes.md"),
        "---\ntype: session\n---\nmore notes",
    )
    .unwrap();

    // MEMORY.md should be excluded
    std::fs::write(dir.path().join("MEMORY.md"), "# Index").unwrap();

    assert_eq!(count_session_files(dir.path()), 2);
}

#[test]
fn test_scan_all_memory_files() {
    let dir = TempDir::new().unwrap();

    std::fs::write(
        dir.path().join("file1.md"),
        "---\ntype: session\n---\ncontent1",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("file2.md"),
        "---\ntype: project\n---\ncontent2",
    )
    .unwrap();
    std::fs::write(dir.path().join("MEMORY.md"), "# Index").unwrap();
    std::fs::write(dir.path().join("not-md.txt"), "text").unwrap();

    let files = scan_all_memory_files(dir.path());
    assert_eq!(files.len(), 2);
    assert!(
        files
            .iter()
            .any(|f| f.filename == "file1.md" && f.file_type == "session")
    );
    assert!(
        files
            .iter()
            .any(|f| f.filename == "file2.md" && f.file_type == "project")
    );
}

#[test]
fn test_load_save_meta() {
    let dir = TempDir::new().unwrap();
    let meta_path = dir.path().join("meta.json");

    // Load non-existent file returns default
    let meta = load_meta(&meta_path);
    assert!(meta.last_run.is_none());
    assert_eq!(meta.files_processed, 0);

    // Save and reload
    let meta = ConsolidationMeta {
        last_run: Some("2026-01-01T00:00:00Z".to_string()),
        files_processed: 5,
    };
    save_meta(&meta_path, &meta);
    let loaded = load_meta(&meta_path);
    assert_eq!(loaded.last_run.unwrap(), "2026-01-01T00:00:00Z");
    assert_eq!(loaded.files_processed, 5);
}

#[test]
fn test_regenerate_index() {
    let dir = TempDir::new().unwrap();

    std::fs::write(
        dir.path().join("alpha.md"),
        "---\ndescription: Alpha file\n---\ncontent",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("beta.md"),
        "---\ndescription: Beta file\n---\ncontent",
    )
    .unwrap();

    regenerate_index(dir.path()).unwrap();

    let index = std::fs::read_to_string(dir.path().join("MEMORY.md")).unwrap();
    assert!(index.contains("# Memory Index"));
    assert!(index.contains("[alpha.md]"));
    assert!(index.contains("Alpha file"));
    assert!(index.contains("[beta.md]"));
    assert!(index.contains("Beta file"));
}

#[test]
fn test_should_consolidate_no_dir() {
    let dir = TempDir::new().unwrap();
    // No memory dir exists
    assert!(!should_consolidate(dir.path()));
}

#[test]
fn test_extract_desc() {
    assert_eq!(
        extract_desc("---\ndescription: \"Hello world\"\n---\nbody"),
        "Hello world"
    );
    assert_eq!(extract_desc("no frontmatter"), "");
}
