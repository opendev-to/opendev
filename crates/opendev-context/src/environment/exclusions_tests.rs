use tempfile::TempDir;

use super::*;

#[test]
fn test_no_patterns_not_excluded() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("AGENTS.md");
    std::fs::write(&file, "content").unwrap();

    assert!(!is_excluded(&file, dir.path(), &[]));
}

#[test]
fn test_filename_glob_match() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("AGENTS.md");
    std::fs::write(&file, "content").unwrap();

    assert!(is_excluded(&file, &dir_path, &["AGENTS.md".to_string()]));
}

#[test]
fn test_wildcard_glob_match() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("test.md");
    std::fs::write(&file, "content").unwrap();

    assert!(is_excluded(&file, &dir_path, &["*.md".to_string()]));
}

#[test]
fn test_no_match_returns_false() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("AGENTS.md");
    std::fs::write(&file, "content").unwrap();

    assert!(!is_excluded(&file, &dir_path, &["*.txt".to_string()]));
}

#[test]
fn test_managed_never_excluded() {
    let path = std::path::Path::new("/etc/opendev/AGENTS.md");
    assert!(!is_excluded(
        path,
        std::path::Path::new("/tmp"),
        &["*.md".to_string()]
    ));
}
