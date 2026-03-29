use super::*;
use tempfile::TempDir;

fn setup() -> (TempDir, PlanIndex) {
    let tmp = TempDir::new().unwrap();
    let index = PlanIndex::new(tmp.path().join("plans"));
    (tmp, index)
}

#[test]
fn test_empty_index() {
    let (_tmp, index) = setup();
    assert!(index.list_all().is_empty());
}

#[test]
fn test_add_and_list() {
    let (_tmp, index) = setup();
    index.add_entry("bold-blazing-badger", Some("sess1"), Some("/home/project"));

    let entries = index.list_all();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "bold-blazing-badger");
    assert_eq!(entries[0].session_id.as_deref(), Some("sess1"));
    assert_eq!(entries[0].project_path.as_deref(), Some("/home/project"));
}

#[test]
fn test_upsert_replaces() {
    let (_tmp, index) = setup();
    index.add_entry("test-plan", Some("old-session"), None);
    index.add_entry("test-plan", Some("new-session"), Some("/project"));

    let entries = index.list_all();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].session_id.as_deref(), Some("new-session"));
    assert_eq!(entries[0].project_path.as_deref(), Some("/project"));
}

#[test]
fn test_get_by_session() {
    let (_tmp, index) = setup();
    index.add_entry("plan-a", Some("sess-a"), None);
    index.add_entry("plan-b", Some("sess-b"), None);

    let found = index.get_by_session("sess-a");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "plan-a");

    assert!(index.get_by_session("nonexistent").is_none());
}

#[test]
fn test_get_by_project() {
    let (_tmp, index) = setup();
    index.add_entry("plan-a", None, Some("/project-x"));
    index.add_entry("plan-b", None, Some("/project-x"));
    index.add_entry("plan-c", None, Some("/project-y"));

    let results = index.get_by_project("/project-x");
    assert_eq!(results.len(), 2);

    let results = index.get_by_project("/project-y");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_remove_entry() {
    let (_tmp, index) = setup();
    index.add_entry("plan-a", None, None);
    index.add_entry("plan-b", None, None);

    index.remove_entry("plan-a");
    let entries = index.list_all();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "plan-b");
}

#[test]
fn test_remove_nonexistent_is_noop() {
    let (_tmp, index) = setup();
    index.add_entry("plan-a", None, None);
    index.remove_entry("nonexistent");
    assert_eq!(index.list_all().len(), 1);
}

#[test]
fn test_corrupted_index_returns_default() {
    let tmp = TempDir::new().unwrap();
    let plans_dir = tmp.path().join("plans");
    std::fs::create_dir_all(&plans_dir).unwrap();
    std::fs::write(plans_dir.join(INDEX_FILE), "not valid json{{{").unwrap();

    let index = PlanIndex::new(&plans_dir);
    assert!(index.list_all().is_empty());
}

#[test]
fn test_persistence_across_instances() {
    let tmp = TempDir::new().unwrap();
    let plans_dir = tmp.path().join("plans");

    {
        let index = PlanIndex::new(&plans_dir);
        index.add_entry("persisted-plan", Some("sess1"), Some("/project"));
    }

    {
        let index = PlanIndex::new(&plans_dir);
        let entries = index.list_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "persisted-plan");
    }
}
