use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

use crate::event_store::{EventEnvelope, SessionEvent};
use crate::projector::SessionProjector;

fn make_envelope(seq: u64, event: &SessionEvent) -> EventEnvelope {
    EventEnvelope::new("test-session", seq, event)
}

fn session_created_event() -> SessionEvent {
    SessionEvent::SessionCreated {
        id: "sess-001".to_string(),
        working_directory: Some("/tmp/project".to_string()),
        channel: "cli".to_string(),
        title: Some("Test Session".to_string()),
        parent_id: None,
        metadata: HashMap::new(),
    }
}

#[test]
fn test_project_from_session_created() {
    let event = session_created_event();
    let envelope = make_envelope(0, &event);

    let session = SessionProjector::project_from_events(&[envelope]).unwrap();
    assert_eq!(session.id, "sess-001");
    assert_eq!(session.working_directory.as_deref(), Some("/tmp/project"));
    assert_eq!(session.channel, "cli");
    assert_eq!(
        session.metadata.get("title").and_then(|v| v.as_str()),
        Some("Test Session")
    );
    assert!(session.messages.is_empty());
}

#[test]
fn test_project_empty_events_errors() {
    let result = SessionProjector::project_from_events(&[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[test]
fn test_project_missing_session_created_errors() {
    let event = SessionEvent::MessageAdded {
        role: "user".to_string(),
        content: "hello".to_string(),
        timestamp: Utc::now(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
    };
    let envelope = make_envelope(0, &event);

    let result = SessionProjector::project_from_events(&[envelope]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("SessionCreated"));
}

#[test]
fn test_apply_message_added() {
    let created = session_created_event();
    let msg = SessionEvent::MessageAdded {
        role: "user".to_string(),
        content: "Hello world".to_string(),
        timestamp: Utc::now(),
        tool_calls: vec![],
        tokens: Some(10),
        thinking_trace: None,
        reasoning_content: None,
    };

    let events = vec![make_envelope(0, &created), make_envelope(1, &msg)];
    let session = SessionProjector::project_from_events(&events).unwrap();

    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].content, "Hello world");
    assert_eq!(session.messages[0].role.to_string(), "user");
    assert_eq!(session.messages[0].tokens, Some(10));
}

#[test]
fn test_apply_message_edited() {
    let created = session_created_event();
    let msg = SessionEvent::MessageAdded {
        role: "assistant".to_string(),
        content: "Original".to_string(),
        timestamp: Utc::now(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
    };
    let edit = SessionEvent::MessageEdited {
        seq: 0,
        content: "Edited content".to_string(),
    };

    let events = vec![
        make_envelope(0, &created),
        make_envelope(1, &msg),
        make_envelope(2, &edit),
    ];
    let session = SessionProjector::project_from_events(&events).unwrap();

    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].content, "Edited content");
}

#[test]
fn test_apply_title_changed() {
    let created = session_created_event();
    let title = SessionEvent::TitleChanged {
        title: "New Title".to_string(),
    };

    let events = vec![make_envelope(0, &created), make_envelope(1, &title)];
    let session = SessionProjector::project_from_events(&events).unwrap();

    assert_eq!(
        session.metadata.get("title").and_then(|v| v.as_str()),
        Some("New Title")
    );
}

#[test]
fn test_apply_session_archived_unarchived() {
    let created = session_created_event();
    let archived = SessionEvent::SessionArchived {
        time_archived: Utc::now(),
    };
    let unarchived = SessionEvent::SessionUnarchived;

    // Archive
    let events = vec![make_envelope(0, &created), make_envelope(1, &archived)];
    let session = SessionProjector::project_from_events(&events).unwrap();
    assert!(session.is_archived());

    // Unarchive
    let events = vec![
        make_envelope(0, &session_created_event()),
        make_envelope(
            1,
            &SessionEvent::SessionArchived {
                time_archived: Utc::now(),
            },
        ),
        make_envelope(2, &unarchived),
    ];
    let session = SessionProjector::project_from_events(&events).unwrap();
    assert!(!session.is_archived());
}

#[test]
fn test_apply_file_change_recorded() {
    use opendev_models::file_change::{FileChange, FileChangeType};

    let created = session_created_event();
    let fc = FileChange::new(FileChangeType::Created, "src/main.rs".to_string());
    let event = SessionEvent::FileChangeRecorded { file_change: fc };

    let events = vec![make_envelope(0, &created), make_envelope(1, &event)];
    let session = SessionProjector::project_from_events(&events).unwrap();

    assert_eq!(session.file_changes.len(), 1);
    assert_eq!(session.file_changes[0].file_path, "src/main.rs");
}

#[test]
fn test_apply_metadata_updated() {
    let created = session_created_event();
    let meta = SessionEvent::MetadataUpdated {
        key: "model".to_string(),
        value: json!("gpt-4"),
    };

    let events = vec![make_envelope(0, &created), make_envelope(1, &meta)];
    let session = SessionProjector::project_from_events(&events).unwrap();

    assert_eq!(
        session.metadata.get("model").and_then(|v| v.as_str()),
        Some("gpt-4")
    );
}

#[test]
fn test_project_full_sequence() {
    use opendev_models::file_change::{FileChange, FileChangeType};

    let events = vec![
        make_envelope(0, &session_created_event()),
        make_envelope(
            1,
            &SessionEvent::MessageAdded {
                role: "user".to_string(),
                content: "Fix the bug".to_string(),
                timestamp: Utc::now(),
                tool_calls: vec![],
                tokens: Some(5),
                thinking_trace: None,
                reasoning_content: None,
            },
        ),
        make_envelope(
            2,
            &SessionEvent::MessageAdded {
                role: "assistant".to_string(),
                content: "I'll fix it".to_string(),
                timestamp: Utc::now(),
                tool_calls: vec![],
                tokens: Some(8),
                thinking_trace: Some("thinking...".to_string()),
                reasoning_content: None,
            },
        ),
        make_envelope(
            3,
            &SessionEvent::TitleChanged {
                title: "Bug Fix Session".to_string(),
            },
        ),
        make_envelope(
            4,
            &SessionEvent::FileChangeRecorded {
                file_change: FileChange::new(FileChangeType::Modified, "src/lib.rs".to_string()),
            },
        ),
        make_envelope(
            5,
            &SessionEvent::MetadataUpdated {
                key: "priority".to_string(),
                value: json!("high"),
            },
        ),
    ];

    let session = SessionProjector::project_from_events(&events).unwrap();

    assert_eq!(session.id, "sess-001");
    assert_eq!(session.messages.len(), 2);
    assert_eq!(session.messages[0].content, "Fix the bug");
    assert_eq!(session.messages[1].content, "I'll fix it");
    assert_eq!(
        session.messages[1].thinking_trace.as_deref(),
        Some("thinking...")
    );
    assert_eq!(
        session.metadata.get("title").and_then(|v| v.as_str()),
        Some("Bug Fix Session")
    );
    assert_eq!(session.file_changes.len(), 1);
    assert_eq!(
        session.metadata.get("priority").and_then(|v| v.as_str()),
        Some("high")
    );
}
