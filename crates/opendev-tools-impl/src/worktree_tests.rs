use super::*;

#[test]
fn test_random_name_format() {
    let name = random_name();
    assert!(name.contains('-'), "random name should contain a hyphen");
    let parts: Vec<&str> = name.split('-').collect();
    assert_eq!(parts.len(), 2, "should have exactly two parts");
}

#[test]
fn test_parse_porcelain_empty() {
    let result = parse_porcelain_output("");
    assert!(result.is_empty());
}

#[test]
fn test_parse_porcelain_single_main() {
    let output = "\
worktree /home/user/project
HEAD abc123def456
branch refs/heads/main
";
    let result = parse_porcelain_output(output);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, "/home/user/project");
    assert_eq!(result[0].branch, "main");
    assert_eq!(result[0].commit, "abc123def456");
    assert!(!result[0].is_main);
}

#[test]
fn test_parse_porcelain_bare_worktree() {
    let output = "\
worktree /home/user/project
HEAD abc123
bare
";
    let result = parse_porcelain_output(output);
    assert_eq!(result.len(), 1);
    assert!(result[0].is_main);
    assert_eq!(result[0].branch, "detached");
}

#[test]
fn test_parse_porcelain_multiple() {
    let output = "\
worktree /home/user/project
HEAD aaa111
branch refs/heads/main

worktree /home/user/.opendev/data/worktree/swift-patch
HEAD bbb222
branch refs/heads/worktree-swift-patch

worktree /home/user/.opendev/data/worktree/calm-spike
HEAD ccc333
branch refs/heads/worktree-calm-spike
";
    let result = parse_porcelain_output(output);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].branch, "main");
    assert_eq!(result[1].branch, "worktree-swift-patch");
    assert_eq!(result[2].branch, "worktree-calm-spike");
}

#[test]
fn test_worktree_info_display() {
    let info = WorktreeInfo {
        path: "/tmp/wt".into(),
        branch: "feature-x".into(),
        commit: "abc".into(),
        is_main: false,
    };
    let s = format!("{info}");
    assert!(s.contains("feature-x"));
    assert!(s.contains("/tmp/wt"));
    assert!(!s.contains("(main)"));

    let main_info = WorktreeInfo {
        path: "/tmp/main".into(),
        branch: "main".into(),
        commit: "def".into(),
        is_main: true,
    };
    let s = format!("{main_info}");
    assert!(s.contains("(main)"));
}

#[test]
fn test_manager_new_default_base() {
    let mgr = WorktreeManager::new("/tmp/project");
    assert_eq!(mgr.project_dir(), Path::new("/tmp/project"));
    assert!(
        mgr.worktree_base().to_string_lossy().contains("worktree"),
        "base should contain 'worktree'"
    );
}

#[test]
fn test_manager_with_base() {
    let mgr = WorktreeManager::with_base("/tmp/project", "/tmp/wt-base");
    assert_eq!(mgr.project_dir(), Path::new("/tmp/project"));
    assert_eq!(mgr.worktree_base(), Path::new("/tmp/wt-base"));
}

#[test]
fn test_resolve_worktree_path_by_name() {
    let mgr = WorktreeManager::with_base("/tmp/project", "/tmp/wt-base");
    let resolved = mgr.resolve_worktree_path("my-worktree");
    // Since /tmp/wt-base/my-worktree doesn't exist, falls back to PathBuf::from
    assert_eq!(resolved, PathBuf::from("my-worktree"));
}

#[test]
fn test_track_untrack() {
    let mut mgr = WorktreeManager::with_base("/tmp/project", "/tmp/wt-base");
    assert!(mgr.tracked().is_empty());

    let info = WorktreeInfo {
        path: "/tmp/wt-base/test".into(),
        branch: "wt-test".into(),
        commit: "abc".into(),
        is_main: false,
    };

    mgr.track("test".into(), info.clone());
    assert_eq!(mgr.tracked().len(), 1);
    assert_eq!(mgr.tracked().get("test"), Some(&info));

    let removed = mgr.untrack("test");
    assert_eq!(removed, Some(info));
    assert!(mgr.tracked().is_empty());
}

#[test]
fn test_untrack_nonexistent() {
    let mut mgr = WorktreeManager::with_base("/tmp/project", "/tmp/wt-base");
    assert_eq!(mgr.untrack("nope"), None);
}

#[test]
fn test_worktree_error_display() {
    let e = WorktreeError::NotFound("wt-1".into());
    assert!(e.to_string().contains("wt-1"));

    let e = WorktreeError::AlreadyExists("wt-2".into());
    assert!(e.to_string().contains("wt-2"));

    let e = WorktreeError::GitError("fatal: not a git repo".into());
    assert!(e.to_string().contains("fatal"));
}

#[test]
fn test_parse_porcelain_detached_head() {
    let output = "\
worktree /tmp/detached
HEAD deadbeef
detached
";
    let result = parse_porcelain_output(output);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].branch, "detached");
    assert_eq!(result[0].commit, "deadbeef");
}

#[tokio::test]
async fn test_list_in_non_git_dir() {
    let mgr = WorktreeManager::with_base("/tmp", "/tmp/wt-nonexist");
    let result = mgr.list().await;
    // /tmp is not a git repo, so this should fail
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_in_non_git_dir() {
    let mut mgr = WorktreeManager::with_base("/tmp", "/tmp/wt-create-test");
    let result = mgr.create(Some("test"), None, "HEAD").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_cleanup_in_non_git_dir() {
    let mgr = WorktreeManager::with_base("/tmp", "/tmp/wt-cleanup-test");
    let result = mgr.cleanup().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_remove_in_non_git_dir() {
    let mut mgr = WorktreeManager::with_base("/tmp", "/tmp/wt-remove-test");
    let result = mgr.remove("nonexistent", false).await;
    assert!(result.is_err());
}
