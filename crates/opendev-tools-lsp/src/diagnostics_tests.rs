use super::*;
use tokio::time::{self, Duration};

#[tokio::test]
async fn test_debouncer_batches_within_window() {
    let (debouncer, mut rx) = DiagnosticsDebouncer::with_duration(Duration::from_millis(50));

    let file = PathBuf::from("/test/main.rs");

    // Push 3 diagnostics rapidly.
    debouncer
        .push(Diagnostic {
            file_path: file.clone(),
            line: 1,
            character: 0,
            severity: 1,
            message: "error 1".to_string(),
            source: Some("rustc".to_string()),
        })
        .unwrap();

    debouncer
        .push(Diagnostic {
            file_path: file.clone(),
            line: 5,
            character: 0,
            severity: 2,
            message: "warning 1".to_string(),
            source: Some("rustc".to_string()),
        })
        .unwrap();

    debouncer
        .push(Diagnostic {
            file_path: file.clone(),
            line: 10,
            character: 0,
            severity: 1,
            message: "error 2".to_string(),
            source: Some("rustc".to_string()),
        })
        .unwrap();

    // Wait for debounce window to pass.
    let batch = time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .expect("Should receive within timeout")
        .expect("Should receive a batch");

    assert_eq!(batch.file_path, file);
    assert_eq!(batch.diagnostics.len(), 3);
}

#[tokio::test]
async fn test_debouncer_separate_files() {
    let (debouncer, mut rx) = DiagnosticsDebouncer::with_duration(Duration::from_millis(50));

    let file_a = PathBuf::from("/test/a.rs");
    let file_b = PathBuf::from("/test/b.rs");

    debouncer
        .push(Diagnostic {
            file_path: file_a.clone(),
            line: 1,
            character: 0,
            severity: 1,
            message: "error in a".to_string(),
            source: None,
        })
        .unwrap();

    debouncer
        .push(Diagnostic {
            file_path: file_b.clone(),
            line: 2,
            character: 0,
            severity: 2,
            message: "warning in b".to_string(),
            source: None,
        })
        .unwrap();

    // Collect batches.
    let mut batches = Vec::new();
    for _ in 0..2 {
        if let Ok(Some(batch)) = time::timeout(Duration::from_millis(200), rx.recv()).await {
            batches.push(batch);
        }
    }

    assert_eq!(batches.len(), 2);
    let files: Vec<PathBuf> = batches.iter().map(|b| b.file_path.clone()).collect();
    assert!(files.contains(&file_a));
    assert!(files.contains(&file_b));
}

#[tokio::test]
async fn test_debouncer_coalesces_rapid_updates() {
    let (debouncer, mut rx) = DiagnosticsDebouncer::with_duration(Duration::from_millis(100));

    let file = PathBuf::from("/test/rapid.rs");

    // Push diagnostics with small delays (all within debounce window).
    for i in 0..5 {
        debouncer
            .push(Diagnostic {
                file_path: file.clone(),
                line: i,
                character: 0,
                severity: 1,
                message: format!("error {}", i),
                source: None,
            })
            .unwrap();
        // 10ms between each, all within 100ms window.
        time::sleep(Duration::from_millis(10)).await;
    }

    let batch = time::timeout(Duration::from_millis(300), rx.recv())
        .await
        .expect("Should receive within timeout")
        .expect("Should receive a batch");

    // All 5 should be in one batch.
    assert_eq!(batch.file_path, file);
    assert_eq!(batch.diagnostics.len(), 5);
}

#[tokio::test]
async fn test_debouncer_default_duration() {
    let (debouncer, _rx) = DiagnosticsDebouncer::new();
    assert_eq!(
        debouncer.debounce_duration(),
        Duration::from_millis(DEFAULT_DEBOUNCE_MS)
    );
}

#[tokio::test]
async fn test_debouncer_stop() {
    let (mut debouncer, _rx) = DiagnosticsDebouncer::with_duration(Duration::from_millis(50));
    debouncer.stop();
    // After stopping, push should fail because channel is still open
    // but the task is aborted. The sender is still valid though.
    // This mainly tests that stop doesn't panic.
    assert!(debouncer.task_handle.is_none());
}

#[test]
fn test_diagnostic_clone_and_eq() {
    let d1 = Diagnostic {
        file_path: PathBuf::from("/test.rs"),
        line: 1,
        character: 0,
        severity: 1,
        message: "test".to_string(),
        source: Some("rustc".to_string()),
    };
    let d2 = d1.clone();
    assert_eq!(d1, d2);
}
