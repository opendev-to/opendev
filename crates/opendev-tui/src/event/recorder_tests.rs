use super::*;

#[test]
fn test_event_recorder_roundtrip() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    // Record some events
    {
        let mut recorder = EventRecorder::new(&path).unwrap();
        recorder.record(&AppEvent::AgentStarted);
        recorder.record(&AppEvent::AgentChunk("hello".to_string()));
        recorder.record(&AppEvent::ToolStarted {
            tool_id: "t1".to_string(),
            tool_name: "bash".to_string(),
            args: {
                let mut m = std::collections::HashMap::new();
                m.insert("command".to_string(), serde_json::json!("echo hi"));
                m
            },
        });
        recorder.record(&AppEvent::AgentFinished);
        recorder.record(&AppEvent::Quit);
    }

    // Load and verify
    let events = load_recorded_events(&path).unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(events[0].variant, "AgentStarted");
    assert_eq!(events[1].variant, "AgentChunk");
    assert_eq!(events[2].variant, "ToolStarted");
    assert_eq!(events[3].variant, "AgentFinished");
    assert_eq!(events[4].variant, "Quit");

    // Verify reconstruction
    assert!(matches!(
        events[0].to_app_event().unwrap(),
        AppEvent::AgentStarted
    ));
    assert!(matches!(
        events[1].to_app_event().unwrap(),
        AppEvent::AgentChunk(_)
    ));
    assert!(matches!(events[4].to_app_event().unwrap(), AppEvent::Quit));
}

#[test]
fn test_recorded_event_sequence_numbers() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let mut recorder = EventRecorder::new(&path).unwrap();
    recorder.record(&AppEvent::Tick);
    recorder.record(&AppEvent::Tick);
    recorder.record(&AppEvent::Tick);
    drop(recorder);

    let events = load_recorded_events(&path).unwrap();
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[1].seq, 2);
    assert_eq!(events[2].seq, 3);
    // Timestamps should be monotonically non-decreasing
    assert!(events[1].timestamp_ms >= events[0].timestamp_ms);
    assert!(events[2].timestamp_ms >= events[1].timestamp_ms);
}

#[test]
fn test_subagent_event_roundtrip() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let event = AppEvent::SubagentFinished {
        subagent_id: "sa-1".to_string(),
        subagent_name: "explorer".to_string(),
        success: true,
        result_summary: "Found 3 files".to_string(),
        tool_call_count: 5,
        shallow_warning: None,
    };

    {
        let mut recorder = EventRecorder::new(&path).unwrap();
        recorder.record(&event);
    }

    let events = load_recorded_events(&path).unwrap();
    assert_eq!(events.len(), 1);
    let reconstructed = events[0].to_app_event().unwrap();
    match reconstructed {
        AppEvent::SubagentFinished {
            subagent_id,
            subagent_name,
            success,
            result_summary,
            tool_call_count,
            shallow_warning,
        } => {
            assert_eq!(subagent_id, "sa-1");
            assert_eq!(subagent_name, "explorer");
            assert!(success);
            assert_eq!(result_summary, "Found 3 files");
            assert_eq!(tool_call_count, 5);
            assert!(shallow_warning.is_none());
        }
        _ => panic!("Wrong event variant"),
    }
}

#[test]
fn test_terminal_events_not_reconstructed() {
    let recorded = RecordedEvent {
        seq: 1,
        timestamp_ms: 0,
        variant: "Terminal".to_string(),
        payload: serde_json::json!("some debug string"),
    };
    assert!(recorded.to_app_event().is_none());
}
