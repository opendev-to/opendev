use super::*;

#[test]
fn test_parse_porcelain_output() {
    let output = "\
worktree /home/user/project
HEAD abc123def456
branch refs/heads/main

worktree /home/user/project-wt1
HEAD def789abc012
branch refs/heads/feature/foo

";
    let worktrees = parse_porcelain_output(output);
    assert_eq!(worktrees.len(), 2);
    assert_eq!(worktrees[0].path, PathBuf::from("/home/user/project"));
    assert_eq!(worktrees[0].branch, "main");
    assert!(worktrees[0].is_main);
    assert_eq!(worktrees[1].branch, "feature/foo");
    assert!(!worktrees[1].is_main);
}

#[test]
fn test_parse_empty_output() {
    let worktrees = parse_porcelain_output("");
    assert!(worktrees.is_empty());
}

#[test]
fn test_generate_short_id() {
    let id1 = generate_short_id();
    assert_eq!(id1.len(), 8);
    // IDs should contain hex chars
    assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_worktree_manager_creation() {
    let mgr = WorktreeManager::new(Path::new("/tmp/test-repo"));
    assert_eq!(mgr.repo_root, PathBuf::from("/tmp/test-repo"));
}

#[test]
fn test_worktrees_dir() {
    let mgr = WorktreeManager::new(Path::new("/project"));
    let dir = mgr.worktrees_dir();
    assert!(dir.to_string_lossy().contains("opendev-worktrees"));
}

#[test]
fn test_debug_format() {
    let mgr = WorktreeManager::new(Path::new("/project"));
    let s = format!("{:?}", mgr);
    assert!(s.contains("WorktreeManager"));
}
