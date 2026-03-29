use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_scan_top_level_dirs_basic() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("crates")).unwrap();
    fs::create_dir(tmp.path().join("docs")).unwrap();
    fs::create_dir(tmp.path().join("web-ui")).unwrap();
    // These should be filtered out
    fs::create_dir(tmp.path().join(".git")).unwrap();
    fs::create_dir(tmp.path().join("node_modules")).unwrap();
    fs::create_dir(tmp.path().join("target")).unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "").unwrap();

    let result = scan_top_level_dirs(tmp.path());
    assert!(result.contains("crates/"), "got: {result}");
    assert!(result.contains("docs/"), "got: {result}");
    assert!(result.contains("web-ui/"), "got: {result}");
    assert!(!result.contains(".git"), "should exclude .git");
    assert!(
        !result.contains("node_modules"),
        "should exclude node_modules"
    );
    assert!(!result.contains("target"), "should exclude target");
    assert!(!result.contains("Cargo.toml"), "should exclude files");
}

#[test]
fn test_scan_project_structure_basic() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("crates/foo")).unwrap();
    fs::write(tmp.path().join("crates/foo/lib.rs"), "").unwrap();
    fs::write(tmp.path().join("README.md"), "").unwrap();

    let result = scan_project_structure(tmp.path(), 3);
    assert!(result.contains("crates/"), "got: {result}");
    assert!(result.contains("foo/"), "got: {result}");
    assert!(result.contains("lib.rs"), "got: {result}");
    assert!(result.contains("README.md"), "got: {result}");
}

#[test]
fn test_scan_project_structure_skips_noise() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::create_dir(tmp.path().join("node_modules")).unwrap();
    fs::create_dir(tmp.path().join("target")).unwrap();

    let result = scan_project_structure(tmp.path(), 3);
    assert!(result.contains("src/"), "got: {result}");
    assert!(!result.contains("node_modules"), "got: {result}");
    assert!(!result.contains("target"), "got: {result}");
}

#[test]
fn test_scan_empty_dir() {
    let tmp = TempDir::new().unwrap();
    let result = scan_project_structure(tmp.path(), 3);
    assert!(result.is_empty());
    let result = scan_top_level_dirs(tmp.path());
    assert!(result.is_empty());
}
