use super::*;
use tempfile::TempDir;

#[test]
fn test_always_ignored() {
    assert!(GitIgnoreParser::is_always_ignored("node_modules"));
    assert!(GitIgnoreParser::is_always_ignored(".git"));
    assert!(GitIgnoreParser::is_always_ignored("__pycache__"));
    assert!(!GitIgnoreParser::is_always_ignored("src"));
}

#[test]
fn test_simple_match() {
    assert!(simple_match("*.rs", "main.rs"));
    assert!(simple_match("*.rs", "lib.rs"));
    assert!(!simple_match("*.rs", "main.py"));
    assert!(simple_match("test_*", "test_foo"));
    assert!(simple_match("?oo", "foo"));
    assert!(!simple_match("?oo", "fooo"));
}

#[test]
fn test_matches_pattern_no_slash() {
    assert!(matches_pattern("*.log", "debug.log"));
    assert!(matches_pattern("*.log", "src/debug.log"));
}

#[test]
fn test_matches_pattern_doublestar() {
    assert!(matches_pattern("**/test", "src/test"));
    assert!(matches_pattern("src/**", "src/foo/bar"));
}

#[test]
fn test_parser_with_gitignore() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create .gitignore
    std::fs::write(root.join(".gitignore"), "*.log\nbuild/\n").unwrap();

    // Create some files
    std::fs::write(root.join("main.rs"), "").unwrap();
    std::fs::write(root.join("debug.log"), "").unwrap();
    std::fs::create_dir(root.join("build")).unwrap();

    let parser = GitIgnoreParser::new(&root);

    assert!(parser.is_ignored(&root.join("debug.log")));
    assert!(parser.is_ignored(&root.join("build")));
    assert!(!parser.is_ignored(&root.join("main.rs")));
}

#[test]
fn test_always_ignored_dirs() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    std::fs::create_dir_all(root.join("node_modules/foo")).unwrap();
    std::fs::write(root.join("node_modules/foo/bar.js"), "").unwrap();

    let parser = GitIgnoreParser::new(&root);
    assert!(parser.is_ignored(&root.join("node_modules/foo/bar.js")));
}

#[test]
fn test_negation_pattern() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    std::fs::write(root.join(".gitignore"), "*.log\n!important.log\n").unwrap();
    std::fs::write(root.join("debug.log"), "").unwrap();
    std::fs::write(root.join("important.log"), "").unwrap();

    let parser = GitIgnoreParser::new(&root);
    assert!(parser.is_ignored(&root.join("debug.log")));
    assert!(!parser.is_ignored(&root.join("important.log")));
}

#[test]
fn test_empty_gitignore() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join(".gitignore"), "# Just comments\n\n").unwrap();

    let parser = GitIgnoreParser::new(root);
    assert!(!parser.is_ignored(&root.join("foo.txt")));
}

#[test]
fn test_relative_path() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(root.join("test.log"), "").unwrap();

    let parser = GitIgnoreParser::new(root);
    assert!(parser.is_ignored(Path::new("test.log")));
}

#[test]
fn test_path_outside_root() {
    let tmp = TempDir::new().unwrap();
    let parser = GitIgnoreParser::new(tmp.path());
    assert!(!parser.is_ignored(Path::new("/completely/different/path")));
}
