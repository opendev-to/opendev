use super::*;
use std::fs;

#[test]
fn test_format_file_size_bytes() {
    assert_eq!(format_file_size(42), "42 B");
}

#[test]
fn test_format_file_size_kb() {
    assert_eq!(format_file_size(2048), "2.0 KB");
}

#[test]
fn test_format_file_size_mb() {
    assert_eq!(format_file_size(1_500_000), "1.4 MB");
}

#[test]
fn test_finder_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let finder = FileFinder::new(dir.path().to_path_buf());
    let results = finder.find_files("", 50);
    assert!(results.is_empty());
}

#[test]
fn test_finder_finds_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("foo.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("bar.txt"), "hello").unwrap();

    let finder = FileFinder::new(dir.path().to_path_buf());
    let results = finder.find_files("", 50);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_finder_query_filter() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.rs"), "").unwrap();
    fs::write(dir.path().join("lib.rs"), "").unwrap();
    fs::write(dir.path().join("readme.md"), "").unwrap();

    let finder = FileFinder::new(dir.path().to_path_buf());
    let results = finder.find_files("rs", 50);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_finder_excludes_git() {
    let dir = tempfile::tempdir().unwrap();
    let git_dir = dir.path().join(".git");
    fs::create_dir(&git_dir).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main").unwrap();
    fs::write(dir.path().join("visible.txt"), "").unwrap();

    let finder = FileFinder::new(dir.path().to_path_buf());
    let results = finder.find_files("", 100);
    // Should only find visible.txt, not .git/HEAD
    for r in &results {
        assert!(
            !r.to_string_lossy().contains(".git"),
            "should not contain .git paths: {:?}",
            r
        );
    }
}

#[test]
fn test_finder_max_results() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..20 {
        fs::write(dir.path().join(format!("file_{:02}.txt", i)), "").unwrap();
    }

    let finder = FileFinder::new(dir.path().to_path_buf());
    let results = finder.find_files("", 5);
    assert_eq!(results.len(), 5);
}

#[test]
fn test_finder_case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("MyFile.TXT"), "").unwrap();

    let finder = FileFinder::new(dir.path().to_path_buf());
    let results = finder.find_files("myfile", 50);
    assert_eq!(results.len(), 1);
}

#[test]
fn test_finder_respects_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    // The `ignore` crate requires a .git directory to recognise the repo
    // and honour .gitignore rules.
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
    fs::write(dir.path().join("app.rs"), "").unwrap();
    fs::write(dir.path().join("debug.log"), "").unwrap();

    let finder = FileFinder::new(dir.path().to_path_buf());
    let results = finder.find_files("", 100);
    // .gitignore itself may appear, but debug.log should not
    let has_log = results
        .iter()
        .any(|p| p.to_string_lossy().contains("debug.log"));
    assert!(!has_log, "debug.log should be excluded by .gitignore");
}
