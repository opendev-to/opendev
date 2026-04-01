use std::process::Command;

use std::path::PathBuf;

use tempfile::TempDir;

use super::*;

// Helper to remove UNC paths on Windows which confuse Git
fn normalize_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if s.starts_with(r"\\?\") {
            return PathBuf::from(s[4..].to_string());
        }
    }
    path
}

/// Create a temporary git repo for testing.
fn create_test_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let repo = normalize_path(dir.path().canonicalize().unwrap());

    Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&repo)
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&repo)
        .output()
        .unwrap();

    // Create initial commit
    std::fs::write(repo.join("README.md"), "# Test\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&repo)
        .output()
        .unwrap();

    dir
}

#[test]
fn test_create_worktree() {
    let repo_dir = create_test_repo();
    let repo = normalize_path(repo_dir.path().canonicalize().unwrap());
    let wt_base = repo.join(".opendev/worktrees");
    let mgr = WorktreeManager::new(wt_base.clone());

    let info = mgr.create(&repo, "agent-12345678").unwrap();
    assert!(info.path.exists());
    assert_eq!(info.branch, "opendev/agent-agent-12");
    assert!(info.path.join("README.md").exists());
}

#[test]
fn test_cleanup_no_changes() {
    let repo_dir = create_test_repo();
    let repo = normalize_path(repo_dir.path().canonicalize().unwrap());
    let wt_base = repo.join(".opendev/worktrees");
    let mgr = WorktreeManager::new(wt_base);

    let info = mgr.create(&repo, "agent-abcdefgh").unwrap();
    assert!(info.path.exists());

    mgr.cleanup("agent-abcdefgh", &repo, false).unwrap();
    assert!(!info.path.exists());
}

#[test]
fn test_cleanup_with_changes_preserves() {
    let repo_dir = create_test_repo();
    let repo = normalize_path(repo_dir.path().canonicalize().unwrap());
    let wt_base = repo.join(".opendev/worktrees");
    let mgr = WorktreeManager::new(wt_base);

    let info = mgr.create(&repo, "agent-changeme1").unwrap();
    // Simulate a change
    std::fs::write(info.path.join("new_file.txt"), "changed").unwrap();

    mgr.cleanup("agent-changeme1", &repo, true).unwrap();
    assert!(info.path.exists()); // preserved because has_changes=true
}

#[test]
fn test_has_changes() {
    let repo_dir = create_test_repo();
    let repo = normalize_path(repo_dir.path().canonicalize().unwrap());
    let wt_base = repo.join(".opendev/worktrees");
    let mgr = WorktreeManager::new(wt_base);

    let info = mgr.create(&repo, "agent-checkchg1").unwrap();

    assert!(!mgr.has_changes("agent-checkchg1", &repo));

    std::fs::write(info.path.join("change.txt"), "data").unwrap();
    assert!(mgr.has_changes("agent-checkchg1", &repo));
}

#[test]
fn test_list_worktrees() {
    let repo_dir = create_test_repo();
    let repo = normalize_path(repo_dir.path().canonicalize().unwrap());
    let wt_base = repo.join(".opendev/worktrees");
    let mgr = WorktreeManager::new(wt_base);

    mgr.create(&repo, "aaaa1111bbbb").unwrap();
    mgr.create(&repo, "cccc2222dddd").unwrap();

    let list = mgr.list();
    assert_eq!(list.len(), 2);
}
