use super::*;

#[test]
fn test_task_progress_creation() {
    let progress = TaskProgress {
        description: "Thinking".to_string(),
        elapsed_secs: 5,
        token_display: Some("1.2k tokens".to_string()),
        interrupted: false,
        started_at: std::time::Instant::now(),
    };
    assert_eq!(progress.description, "Thinking");
}

#[test]
fn test_format_final_status_completed() {
    let progress = TaskProgress {
        description: "Thinking".to_string(),
        elapsed_secs: 3,
        token_display: None,
        interrupted: false,
        started_at: std::time::Instant::now(),
    };
    let status = format_final_status(&progress);
    assert!(status.contains("completed in 3s"));
    assert!(status.starts_with('\u{23fa}'));
}

#[test]
fn test_format_final_status_interrupted() {
    let progress = TaskProgress {
        description: "Running".to_string(),
        elapsed_secs: 7,
        token_display: Some("2.5k tokens".to_string()),
        interrupted: true,
        started_at: std::time::Instant::now(),
    };
    let status = format_final_status(&progress);
    assert!(status.contains("interrupted in 7s"));
    assert!(status.contains("2.5k tokens"));
    assert!(status.starts_with('\u{23f9}'));
}
