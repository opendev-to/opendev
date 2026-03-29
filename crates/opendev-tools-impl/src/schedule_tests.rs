use super::*;
use tempfile::TempDir;

#[test]
fn test_load_save_schedules() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("schedules.json");

    let schedules = vec![ScheduleEntry {
        id: "abc".to_string(),
        description: "test".to_string(),
        command: "echo hi".to_string(),
        created_at: Utc::now(),
        run_at: None,
        interval_secs: Some(60),
        enabled: true,
    }];

    save_schedules(&path, &schedules).unwrap();
    let loaded = load_schedules(&path);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "abc");
}

#[test]
fn test_load_nonexistent() {
    let schedules = load_schedules(std::path::Path::new("/nonexistent/path.json"));
    assert!(schedules.is_empty());
}

#[tokio::test]
async fn test_schedule_missing_action() {
    let tool = ScheduleTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
}
