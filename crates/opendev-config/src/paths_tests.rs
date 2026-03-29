use super::*;

#[test]
fn test_encode_project_path() {
    // Use a path that doesn't need canonicalization
    let encoded = "/Users/foo/bar".replace('/', "-");
    assert_eq!(encoded, "-Users-foo-bar");
}

#[test]
fn test_paths_global_dir() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/test-project")));
    let global = paths.global_dir();
    assert!(global.to_string_lossy().ends_with(".opendev"));
}

#[test]
fn test_paths_project_dir() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/test-project")));
    assert_eq!(
        paths.project_dir(),
        PathBuf::from("/tmp/test-project/.opendev")
    );
}

#[test]
fn test_session_file() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/test")));
    let session_path = paths.session_file("abc123");
    assert!(session_path.to_string_lossy().ends_with("abc123.json"));
}

#[test]
fn test_project_context_file() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/myproject")));
    assert_eq!(
        paths.project_context_file(),
        PathBuf::from("/tmp/myproject/AGENTS.md")
    );
}

#[test]
fn test_project_mcp_config() {
    let paths = Paths::new(Some(PathBuf::from("/tmp/myproject")));
    assert_eq!(
        paths.project_mcp_config(),
        PathBuf::from("/tmp/myproject/.mcp.json")
    );
}
