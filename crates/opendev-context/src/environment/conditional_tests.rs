use std::path::PathBuf;

use tempfile::TempDir;

use super::*;

#[test]
fn test_none_globs_always_applies() {
    assert!(rule_applies(None, &[], std::path::Path::new("/tmp")));
}

#[test]
fn test_empty_globs_always_applies() {
    let globs: Vec<String> = vec![];
    assert!(rule_applies(
        Some(&globs),
        &[],
        std::path::Path::new("/tmp")
    ));
}

#[test]
fn test_matching_file() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("src").join("main.rs");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "fn main() {}").unwrap();

    let globs = vec!["src/**/*.rs".to_string()];
    assert!(rule_applies(Some(&globs), &[file], &dir_path));
}

#[test]
fn test_non_matching_file() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("docs").join("README.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# README").unwrap();

    let globs = vec!["src/**/*.rs".to_string()];
    assert!(!rule_applies(Some(&globs), &[file], &dir_path));
}

#[test]
fn test_multiple_globs_one_matches() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let file = dir_path.join("tests").join("test_main.rs");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "// test").unwrap();

    let globs = vec!["src/**/*.rs".to_string(), "tests/**/*.rs".to_string()];
    assert!(rule_applies(Some(&globs), &[file], &dir_path));
}

#[test]
fn test_no_active_files_returns_false() {
    let globs = vec!["src/**/*.rs".to_string()];
    let files: Vec<PathBuf> = vec![];
    assert!(!rule_applies(
        Some(&globs),
        &files,
        std::path::Path::new("/tmp")
    ));
}
