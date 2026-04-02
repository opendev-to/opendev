use super::*;
use tempfile::TempDir;

fn make_list() -> (TeamTaskList, TempDir) {
    let dir = TempDir::new().unwrap();
    let list = TeamTaskList::new(dir.path().to_path_buf());
    (list, dir)
}

#[test]
fn test_add_and_list_tasks() {
    let (list, _dir) = make_list();
    let task = TeamTask::new("Fix bug", "Reproduce and fix the crash");
    let created = list.add_task("team-a", task).unwrap();
    assert_eq!(created.status, TeamTaskStatus::Pending);

    let tasks = list.list_tasks("team-a").unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Fix bug");
}

#[test]
fn test_claim_task() {
    let (list, _dir) = make_list();
    let task = TeamTask::new("Implement feature", "Build the auth module");
    let created = list.add_task("team-b", task).unwrap();

    let claimed = list.claim_task("team-b", &created.id, "alice").unwrap();
    assert!(claimed.is_some());
    let claimed = claimed.unwrap();
    assert_eq!(claimed.status, TeamTaskStatus::InProgress);
    assert_eq!(claimed.assigned_to.as_deref(), Some("alice"));
}

#[test]
fn test_claim_already_claimed_returns_none() {
    let (list, _dir) = make_list();
    let task = TeamTask::new("Write docs", "Document API");
    let created = list.add_task("team-c", task).unwrap();

    list.claim_task("team-c", &created.id, "alice").unwrap();
    let second = list.claim_task("team-c", &created.id, "bob").unwrap();
    assert!(second.is_none());
}

#[test]
fn test_complete_task() {
    let (list, _dir) = make_list();
    let task = TeamTask::new("Run tests", "CI pipeline");
    let created = list.add_task("team-d", task).unwrap();
    list.claim_task("team-d", &created.id, "ci").unwrap();

    let completed = list.complete_task("team-d", &created.id, true).unwrap();
    assert!(completed.is_some());
    assert_eq!(completed.unwrap().status, TeamTaskStatus::Completed);
}

#[test]
fn test_dependency_blocks_claim() {
    let (list, _dir) = make_list();
    let dep = TeamTask::new("Step 1", "Must happen first");
    let dep_id = dep.id.clone();
    list.add_task("team-e", dep).unwrap();

    let mut dep2 = TeamTask::new("Step 2", "Depends on step 1");
    dep2.dependencies.push(dep_id.clone());
    let dep2_created = list.add_task("team-e", dep2).unwrap();

    // Can't claim Step 2 while Step 1 is still Pending
    let result = list.claim_task("team-e", &dep2_created.id, "bob").unwrap();
    assert!(result.is_none());

    // Complete Step 1
    list.claim_task("team-e", &dep_id, "alice").unwrap();
    list.complete_task("team-e", &dep_id, true).unwrap();

    // Now Step 2 can be claimed
    let result = list.claim_task("team-e", &dep2_created.id, "bob").unwrap();
    assert!(result.is_some());
}

#[test]
fn test_persistence() {
    let dir = TempDir::new().unwrap();
    {
        let list = TeamTaskList::new(dir.path().to_path_buf());
        list.add_task("team-f", TeamTask::new("Persisted", "Should survive reload"))
            .unwrap();
    }
    // Re-open and check
    let list2 = TeamTaskList::new(dir.path().to_path_buf());
    let tasks = list2.list_tasks("team-f").unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Persisted");
}
