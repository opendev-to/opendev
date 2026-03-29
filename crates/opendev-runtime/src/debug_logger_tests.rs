use super::*;

#[test]
fn test_noop_logger() {
    let logger = SessionDebugLogger::noop();
    assert!(!logger.is_enabled());
    assert!(logger.file_path().is_none());
    // Should not panic
    logger.log("test", "test", serde_json::json!({"key": "value"}));
}

#[test]
fn test_active_logger() {
    let tmp = tempfile::TempDir::new().unwrap();
    let logger = SessionDebugLogger::new(tmp.path(), "test123");
    assert!(logger.is_enabled());
    assert!(logger.file_path().is_some());

    logger.log("event1", "comp1", serde_json::json!({"foo": "bar"}));
    logger.log("event2", "comp2", serde_json::json!({"count": 42}));

    let content = std::fs::read_to_string(logger.file_path().unwrap()).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    let entry: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(entry["event"], "event1");
    assert_eq!(entry["component"], "comp1");
    assert_eq!(entry["data"]["foo"], "bar");
}

#[test]
fn test_truncation() {
    let long_string = "x".repeat(300);
    let data = serde_json::json!({"msg": long_string});
    let truncated = truncate_value(&data);

    let msg = truncated["msg"].as_str().unwrap();
    assert!(msg.len() < 300);
    assert!(msg.contains("300 chars"));
}

#[test]
fn test_nested_truncation() {
    let long = "y".repeat(500);
    let data = serde_json::json!({
        "outer": {
            "inner": long
        }
    });
    let truncated = truncate_value(&data);
    let inner = truncated["outer"]["inner"].as_str().unwrap();
    assert!(inner.contains("500 chars"));
}

#[test]
fn test_elapsed_ms() {
    let tmp = tempfile::TempDir::new().unwrap();
    let logger = SessionDebugLogger::new(tmp.path(), "elapsed_test");

    std::thread::sleep(std::time::Duration::from_millis(10));
    logger.log("test", "test", serde_json::json!({}));

    let content = std::fs::read_to_string(logger.file_path().unwrap()).unwrap();
    let entry: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    let elapsed = entry["elapsed_ms"].as_u64().unwrap();
    assert!(elapsed >= 10);
}
