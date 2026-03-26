//! Integration tests for session history management.
//!
//! Tests full session lifecycle, listing, file locks, and snapshot operations
//! using real filesystem I/O with temp directories.

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use opendev_history::{FileLock, SessionIndex, SessionListing, SessionManager, SnapshotManager};
use opendev_models::{ChatMessage, Role, Session};
use tempfile::TempDir;

fn make_msg(role: Role, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_string(),
        timestamp: Utc::now(),
        metadata: HashMap::new(),
        tool_calls: vec![],
        tokens: None,
        thinking_trace: None,
        reasoning_content: None,
        token_usage: None,
        provenance: None,
    }
}

// ========================================================================
// Session lifecycle: create -> add messages -> save -> reload -> verify
// ========================================================================

/// Full round-trip: create session, add messages, save, reload, verify content.
#[test]
fn session_full_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    // Create
    let session = mgr.create_session();
    let session_id = session.id.clone();
    assert!(!session_id.is_empty());

    // Add messages to current session
    let current = mgr.current_session_mut().unwrap();
    current.messages.push(make_msg(Role::User, "Hello agent"));
    current
        .messages
        .push(make_msg(Role::Assistant, "Hello human"));
    current.messages.push(make_msg(Role::User, "Do something"));

    // Save
    mgr.save_current().unwrap();

    // Reload in a new manager instance
    let mgr2 = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let loaded = mgr2.load_session(&session_id).unwrap();

    assert_eq!(loaded.id, session_id);
    assert_eq!(loaded.messages.len(), 3);
    assert_eq!(loaded.messages[0].content, "Hello agent");
    assert_eq!(loaded.messages[1].content, "Hello human");
    assert_eq!(loaded.messages[2].content, "Do something");
    assert_eq!(loaded.messages[0].role, Role::User);
    assert_eq!(loaded.messages[1].role, Role::Assistant);
}

/// Session with metadata survives save/load.
#[test]
fn session_metadata_persists() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "meta-test".to_string();
    session
        .metadata
        .insert("title".to_string(), serde_json::json!("Test Session"));
    session.working_directory = Some("/tmp/project".to_string());
    session.messages.push(make_msg(Role::User, "hi"));

    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("meta-test").unwrap();
    assert_eq!(
        loaded.metadata.get("title"),
        Some(&serde_json::json!("Test Session"))
    );
    assert_eq!(loaded.working_directory.as_deref(), Some("/tmp/project"));
}

/// Resume session sets it as current.
#[test]
fn resume_sets_current_session() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "resume-target".to_string();
    session.messages.push(make_msg(Role::User, "context"));
    mgr.save_session(&session).unwrap();

    assert!(mgr.current_session().is_none());
    mgr.resume_session("resume-target").unwrap();
    let current = mgr.current_session().unwrap();
    assert_eq!(current.id, "resume-target");
    assert_eq!(current.messages.len(), 1);
}

/// Loading a nonexistent session returns an error.
#[test]
fn load_nonexistent_session_fails() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let result = mgr.load_session("does-not-exist");
    assert!(result.is_err());
}

/// Legacy JSON format (messages embedded in .json, no .jsonl) can be loaded.
#[test]
fn legacy_json_format_loads() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "legacy".to_string();
    session
        .messages
        .push(make_msg(Role::User, "old format msg"));

    // Write as monolithic JSON (legacy)
    let json = serde_json::to_string_pretty(&session).unwrap();
    std::fs::write(tmp.path().join("legacy.json"), json).unwrap();
    // No .jsonl file

    let loaded = mgr.load_session("legacy").unwrap();
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.messages[0].content, "old format msg");
}

// ========================================================================
// Session listing and filtering
// ========================================================================

/// SessionListing returns sessions sorted by updated_at descending.
#[test]
fn listing_returns_sessions() {
    let tmp = TempDir::new().unwrap();
    let listing = SessionListing::new(tmp.path().to_path_buf());
    let index = SessionIndex::new(tmp.path().to_path_buf());

    for i in 0..5 {
        let mut session = Session::new();
        session.id = format!("list-{i}");
        index.upsert_entry(&session).unwrap();
    }

    let sessions = listing.list_sessions(None, false);
    assert_eq!(sessions.len(), 5);
}

/// Find latest session returns the most recently updated.
#[test]
fn listing_find_latest() {
    let tmp = TempDir::new().unwrap();
    let listing = SessionListing::new(tmp.path().to_path_buf());
    let index = SessionIndex::new(tmp.path().to_path_buf());

    for i in 0..3 {
        let mut session = Session::new();
        session.id = format!("latest-{i}");
        index.upsert_entry(&session).unwrap();
    }

    let latest = listing.find_latest_session();
    assert!(latest.is_some());
}

/// Delete removes session files and index entry.
#[test]
fn listing_delete_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let listing = SessionListing::new(tmp.path().to_path_buf());

    // Create and save a session
    let mut session = Session::new();
    session.id = "to-delete".to_string();
    session.messages.push(make_msg(Role::User, "bye"));
    mgr.save_session(&session).unwrap();

    // Verify files exist
    assert!(tmp.path().join("to-delete.json").exists());
    assert!(tmp.path().join("to-delete.jsonl").exists());

    // Delete
    listing.delete_session("to-delete").unwrap();

    // Files should be gone
    assert!(!tmp.path().join("to-delete.json").exists());
    assert!(!tmp.path().join("to-delete.jsonl").exists());

    // Index should not contain entry
    let index = SessionIndex::new(tmp.path().to_path_buf());
    let idx = index.read_index();
    let has_entry = idx
        .map(|i| i.entries.iter().any(|e| e.session_id == "to-delete"))
        .unwrap_or(false);
    assert!(!has_entry, "deleted session should not be in index");
}

/// Find session by channel and user.
#[test]
fn listing_find_by_channel_user() {
    let tmp = TempDir::new().unwrap();
    let listing = SessionListing::new(tmp.path().to_path_buf());
    let index = SessionIndex::new(tmp.path().to_path_buf());

    let mut session = Session::new();
    session.id = "channel-test".to_string();
    session.channel = "slack".to_string();
    session.channel_user_id = "U123".to_string();
    index.upsert_entry(&session).unwrap();

    let found = listing.find_session_by_channel_user("slack", "U123", None);
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, "channel-test");

    let not_found = listing.find_session_by_channel_user("discord", "U123", None);
    assert!(not_found.is_none());
}

// ========================================================================
// File locks
// ========================================================================

/// Basic lock acquire and release.
#[test]
fn file_lock_acquire_release() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let lock = FileLock::acquire(tmp.path(), Duration::from_secs(5)).unwrap();
    lock.release();
}

/// Lock is released when guard is dropped.
#[test]
fn file_lock_released_on_drop() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    {
        let _lock = FileLock::acquire(tmp.path(), Duration::from_secs(5)).unwrap();
    }
    // If the lock wasn't released, this would deadlock/timeout
    let _lock2 = FileLock::acquire(tmp.path(), Duration::from_secs(1)).unwrap();
}

/// with_file_lock executes closure while holding lock.
#[test]
fn with_file_lock_executes_closure() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let result =
        opendev_history::file_locks::with_file_lock(tmp.path(), Duration::from_secs(5), || 42 + 1)
            .unwrap();
    assert_eq!(result, 43);
}

// ========================================================================
// Session index
// ========================================================================

/// Index upsert creates and updates entries without duplicates.
#[test]
fn index_upsert_no_duplicates() {
    let tmp = TempDir::new().unwrap();
    let index = SessionIndex::new(tmp.path().to_path_buf());

    let mut session = Session::new();
    session.id = "dup-test".to_string();

    index.upsert_entry(&session).unwrap();
    index.upsert_entry(&session).unwrap();
    index.upsert_entry(&session).unwrap();

    let idx = index.read_index().unwrap();
    assert_eq!(idx.entries.len(), 1, "should not create duplicates");
}

/// Index remove deletes correct entry.
#[test]
fn index_remove_entry() {
    let tmp = TempDir::new().unwrap();
    let index = SessionIndex::new(tmp.path().to_path_buf());

    let mut s1 = Session::new();
    s1.id = "keep".to_string();
    let mut s2 = Session::new();
    s2.id = "remove".to_string();

    index.upsert_entry(&s1).unwrap();
    index.upsert_entry(&s2).unwrap();

    index.remove_entry("remove").unwrap();

    let idx = index.read_index().unwrap();
    assert_eq!(idx.entries.len(), 1);
    assert_eq!(idx.entries[0].session_id, "keep");
}

/// Invalid index version returns None.
#[test]
fn index_invalid_version_returns_none() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("sessions-index.json");
    std::fs::write(&index_path, r#"{"version": 999, "entries": []}"#).unwrap();

    let index = SessionIndex::new(tmp.path().to_path_buf());
    assert!(index.read_index().is_none());
}

// ========================================================================
// Snapshot manager
// ========================================================================

/// Snapshot manager initializes with zero snapshots.
#[test]
fn snapshot_manager_starts_empty() {
    let mgr = SnapshotManager::new("/tmp/test-project");
    assert_eq!(mgr.snapshot_count(), 0);
}

/// Project-scoped sessions are isolated from each other.
#[test]
fn project_scoped_session_isolation() {
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();

    let mut mgr_a = SessionManager::new(project_a.path().to_path_buf()).unwrap();
    let mut mgr_b = SessionManager::new(project_b.path().to_path_buf()).unwrap();

    // Create session in project A
    let session_a = mgr_a.create_session();
    let id_a = session_a.id.clone();
    mgr_a
        .current_session_mut()
        .unwrap()
        .messages
        .push(make_msg(Role::User, "Project A message"));
    mgr_a.save_current().unwrap();

    // Create session in project B
    let session_b = mgr_b.create_session();
    let id_b = session_b.id.clone();
    mgr_b
        .current_session_mut()
        .unwrap()
        .messages
        .push(make_msg(Role::User, "Project B message"));
    mgr_b.save_current().unwrap();

    // Sessions are different IDs
    assert_ne!(id_a, id_b);

    // Project A cannot see project B's session
    let result = mgr_a.load_session(&id_b);
    assert!(
        result.is_err(),
        "project A should not see project B's session"
    );

    // Project B cannot see project A's session
    let result = mgr_b.load_session(&id_a);
    assert!(
        result.is_err(),
        "project B should not see project A's session"
    );

    // Each can see their own
    let loaded_a = mgr_a.load_session(&id_a).unwrap();
    assert_eq!(loaded_a.messages[0].content, "Project A message");

    let loaded_b = mgr_b.load_session(&id_b).unwrap();
    assert_eq!(loaded_b.messages[0].content, "Project B message");
}

/// Multiple sessions in the same project directory are independently accessible.
#[test]
fn multiple_sessions_in_same_project() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    // Create first session
    let s1 = mgr.create_session();
    let id1 = s1.id.clone();
    mgr.current_session_mut()
        .unwrap()
        .messages
        .push(make_msg(Role::User, "Session 1"));
    mgr.save_current().unwrap();

    // Create second session (replaces current)
    let s2 = mgr.create_session();
    let id2 = s2.id.clone();
    mgr.current_session_mut()
        .unwrap()
        .messages
        .push(make_msg(Role::User, "Session 2"));
    mgr.save_current().unwrap();

    assert_ne!(id1, id2);

    // Both sessions are loadable
    let loaded1 = mgr.load_session(&id1).unwrap();
    assert_eq!(loaded1.messages.len(), 1);
    assert_eq!(loaded1.messages[0].content, "Session 1");

    let loaded2 = mgr.load_session(&id2).unwrap();
    assert_eq!(loaded2.messages.len(), 1);
    assert_eq!(loaded2.messages[0].content, "Session 2");
}

/// Session with working_directory metadata survives save/load.
#[test]
fn session_working_directory_persists() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "wd-test".to_string();
    session.working_directory = Some("/home/user/project".to_string());
    session.messages.push(make_msg(Role::User, "init"));
    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("wd-test").unwrap();
    assert_eq!(
        loaded.working_directory.as_deref(),
        Some("/home/user/project")
    );
}

/// Session messages preserve all fields (role, content, metadata, tool_calls).
#[test]
fn session_message_fields_preserved() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut msg = make_msg(Role::User, "test message");
    msg.metadata
        .insert("key".to_string(), serde_json::json!("value"));

    let mut session = Session::new();
    session.id = "fields-test".to_string();
    session.messages.push(msg);
    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("fields-test").unwrap();
    assert_eq!(loaded.messages[0].role, Role::User);
    assert_eq!(loaded.messages[0].content, "test message");
    assert_eq!(
        loaded.messages[0].metadata.get("key"),
        Some(&serde_json::json!("value"))
    );
}

/// Snapshot track and patch detect file changes.
/// This test requires git to be installed.
#[test]
fn snapshot_track_and_patch() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    // Initialize a real git repo
    let init_ok = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !init_ok {
        // git not available, skip test
        return;
    }

    std::fs::write(tmp.path().join("file.txt"), "version 1").unwrap();

    let mut mgr = SnapshotManager::new(&project_dir);
    let hash1 = mgr.track();

    if hash1.is_none() {
        // Shadow repo init failed (e.g., CI environment), skip
        return;
    }
    assert_eq!(mgr.snapshot_count(), 1);

    // Modify file
    std::fs::write(tmp.path().join("file.txt"), "version 2").unwrap();

    let changed = mgr.patch(hash1.as_ref().unwrap());
    assert!(
        changed.contains(&"file.txt".to_string()),
        "patch should detect changed file"
    );
}

/// Snapshot diff ignores OpenDev internal overflow artifacts.
#[test]
fn snapshot_diff_ignores_internal_tool_output() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    let init_ok = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !init_ok {
        return;
    }

    std::fs::write(tmp.path().join(".gitignore"), ".opendev/\n").unwrap();
    std::fs::write(tmp.path().join("file.txt"), "baseline\n").unwrap();

    let mut mgr = SnapshotManager::new(&project_dir);
    let Some(hash1) = mgr.track() else {
        return;
    };

    let tool_output_dir = tmp.path().join(".opendev").join("tool-output");
    std::fs::create_dir_all(&tool_output_dir).unwrap();
    std::fs::write(tool_output_dir.join("tool_1_read_file.txt"), "generated\n").unwrap();

    let hash2 = mgr.track().unwrap();
    let stats = mgr.diff_numstat(&hash1, &hash2);
    assert!(
        stats.is_empty(),
        "internal tool-output files should be ignored"
    );
}

/// Snapshot diff still reports real workspace edits.
#[test]
fn snapshot_diff_reports_real_file_changes() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    let init_ok = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !init_ok {
        return;
    }

    std::fs::write(tmp.path().join(".gitignore"), ".opendev/\n").unwrap();
    std::fs::write(tmp.path().join("file.txt"), "before\n").unwrap();

    let mut mgr = SnapshotManager::new(&project_dir);
    let Some(hash1) = mgr.track() else {
        return;
    };

    std::fs::write(tmp.path().join("file.txt"), "before\nafter\n").unwrap();

    let hash2 = mgr.track().unwrap();
    let stats = mgr.diff_numstat(&hash1, &hash2);
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].file_path, "file.txt");
    assert!(stats[0].additions > 0);
}

/// Snapshot diff ignores internal artifacts when mixed with real edits.
#[test]
fn snapshot_diff_mixed_real_and_internal_changes_only_reports_real_files() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().to_string_lossy().to_string();

    let init_ok = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&project_dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !init_ok {
        return;
    }

    std::fs::write(tmp.path().join(".gitignore"), ".opendev/\n").unwrap();
    std::fs::write(tmp.path().join("file.txt"), "before\n").unwrap();

    let mut mgr = SnapshotManager::new(&project_dir);
    let Some(hash1) = mgr.track() else {
        return;
    };

    std::fs::write(tmp.path().join("file.txt"), "before\nafter\n").unwrap();
    let tool_output_dir = tmp.path().join(".opendev").join("tool-output");
    std::fs::create_dir_all(&tool_output_dir).unwrap();
    std::fs::write(tool_output_dir.join("tool_2_read_file.txt"), "generated\n").unwrap();

    let hash2 = mgr.track().unwrap();
    let stats = mgr.diff_numstat(&hash1, &hash2);
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].file_path, "file.txt");
}

// ========================================================================
// Session persistence round-trip tests
// ========================================================================

/// Empty session survives save/load round-trip.
#[test]
fn session_roundtrip_empty() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "empty-rt".to_string();
    // No messages, no metadata beyond defaults
    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("empty-rt").unwrap();
    assert_eq!(loaded.id, "empty-rt");
    assert!(loaded.messages.is_empty());
}

/// Unicode content in messages survives round-trip.
#[test]
fn session_roundtrip_unicode() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let unicode_texts = vec![
        "\u{1F600} Emoji test \u{1F680}\u{1F30D}",
        "\u{4F60}\u{597D}\u{4E16}\u{754C} - Chinese",
        "\u{0410}\u{043B}\u{0435}\u{043A}\u{0441}\u{0430}\u{043D}\u{0434}\u{0440} - Russian",
        "\u{3053}\u{3093}\u{306B}\u{3061}\u{306F} - Japanese",
        "Caf\u{00E9} na\u{00EF}ve r\u{00E9}sum\u{00E9} - French accents",
        "Line1\nLine2\n\tTabbed\n\r\nWindows newline",
    ];

    let mut session = Session::new();
    session.id = "unicode-rt".to_string();
    for text in &unicode_texts {
        session.messages.push(make_msg(Role::User, text));
    }
    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("unicode-rt").unwrap();
    assert_eq!(loaded.messages.len(), unicode_texts.len());
    for (i, text) in unicode_texts.iter().enumerate() {
        assert_eq!(
            loaded.messages[i].content, *text,
            "Unicode mismatch at index {i}"
        );
    }
}

/// Large message count survives round-trip without data loss.
#[test]
fn session_roundtrip_large_message_count() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let msg_count = 500;
    let mut session = Session::new();
    session.id = "large-rt".to_string();
    for i in 0..msg_count {
        let role = if i % 2 == 0 {
            Role::User
        } else {
            Role::Assistant
        };
        session.messages.push(make_msg(
            role,
            &format!(
                "Message number {i} with some content to make it realistic. \
             This includes details about the task at hand."
            ),
        ));
    }
    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("large-rt").unwrap();
    assert_eq!(loaded.messages.len(), msg_count);

    // Spot check first, middle, and last messages
    assert!(loaded.messages[0].content.contains("Message number 0"));
    assert!(loaded.messages[250].content.contains("Message number 250"));
    assert!(loaded.messages[499].content.contains("Message number 499"));

    // Verify roles alternate correctly
    for (i, msg) in loaded.messages.iter().enumerate() {
        let expected_role = if i % 2 == 0 {
            Role::User
        } else {
            Role::Assistant
        };
        assert_eq!(msg.role, expected_role, "Role mismatch at message {i}");
    }
}

/// Sessions with tool results survive round-trip.
#[test]
fn session_roundtrip_with_tool_results() {
    use opendev_models::ToolCall;

    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "tool-rt".to_string();

    // User message
    session
        .messages
        .push(make_msg(Role::User, "Read the config file"));

    // Assistant message with tool call
    let mut assistant_msg = make_msg(Role::Assistant, "I'll read the file.");
    let mut params = HashMap::new();
    params.insert("path".to_string(), serde_json::json!("/etc/config.toml"));
    assistant_msg.tool_calls.push(ToolCall {
        id: "tc-001".to_string(),
        name: "read_file".to_string(),
        parameters: params,
        result: Some(serde_json::json!({
            "success": true,
            "output": "[database]\nhost = localhost\nport = 5432"
        })),
        result_summary: Some("Read 3 lines from config.toml".to_string()),
        timestamp: Utc::now(),
        approved: true,
        error: None,
        nested_tool_calls: vec![],
    });
    session.messages.push(assistant_msg);

    // Another assistant message with a failed tool call
    let mut fail_msg = make_msg(Role::Assistant, "Let me try writing.");
    fail_msg.tool_calls.push(ToolCall {
        id: "tc-002".to_string(),
        name: "write_file".to_string(),
        parameters: HashMap::new(),
        result: None,
        result_summary: None,
        timestamp: Utc::now(),
        approved: false,
        error: Some("Permission denied".to_string()),
        nested_tool_calls: vec![],
    });
    session.messages.push(fail_msg);

    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("tool-rt").unwrap();
    assert_eq!(loaded.messages.len(), 3);

    // Verify tool call data survived
    let tc = &loaded.messages[1].tool_calls[0];
    assert_eq!(tc.id, "tc-001");
    assert_eq!(tc.name, "read_file");
    assert!(tc.result.is_some());
    assert_eq!(
        tc.result_summary.as_deref(),
        Some("Read 3 lines from config.toml")
    );
    assert!(tc.approved);
    assert!(tc.error.is_none());

    // Verify failed tool call
    let fail_tc = &loaded.messages[2].tool_calls[0];
    assert_eq!(fail_tc.id, "tc-002");
    assert!(!fail_tc.approved);
    assert_eq!(fail_tc.error.as_deref(), Some("Permission denied"));
}

/// Session with thinking trace and reasoning content survives round-trip.
#[test]
fn session_roundtrip_thinking_metadata() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "thinking-rt".to_string();

    let mut msg = make_msg(Role::Assistant, "The answer is 42.");
    msg.thinking_trace = Some("Let me reason through this step by step...".to_string());
    msg.reasoning_content = Some("Considering the question carefully...".to_string());
    msg.tokens = Some(150);
    session.messages.push(msg);

    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("thinking-rt").unwrap();
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(
        loaded.messages[0].thinking_trace.as_deref(),
        Some("Let me reason through this step by step...")
    );
    assert_eq!(
        loaded.messages[0].reasoning_content.as_deref(),
        Some("Considering the question carefully...")
    );
    assert_eq!(loaded.messages[0].tokens, Some(150));
}
