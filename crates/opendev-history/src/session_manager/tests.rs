use super::*;
use chrono::Utc;
use opendev_models::{ChatMessage, Role};
use std::collections::HashMap;
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

#[test]
fn test_create_session() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let session = mgr.create_session();
    assert!(!session.id.is_empty());
    assert!(mgr.current_session().is_some());
}

#[test]
fn test_save_and_load_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "test-save-load".to_string();
    session.messages.push(make_msg(Role::User, "hello"));
    session.messages.push(make_msg(Role::Assistant, "hi there"));

    mgr.save_session(&session).unwrap();

    let loaded = mgr.load_session("test-save-load").unwrap();
    assert_eq!(loaded.id, "test-save-load");
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].content, "hello");
    assert_eq!(loaded.messages[1].content, "hi there");
}

#[test]
fn test_save_updates_index() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "indexed-session".to_string();
    mgr.save_session(&session).unwrap();

    let index = mgr.index().read_index().unwrap();
    assert_eq!(index.entries.len(), 1);
    assert_eq!(index.entries[0].session_id, "indexed-session");
}

#[test]
fn test_resume_session() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "resume-test".to_string();
    session.messages.push(make_msg(Role::User, "hi"));
    mgr.save_session(&session).unwrap();

    mgr.resume_session("resume-test").unwrap();
    let current = mgr.current_session().unwrap();
    assert_eq!(current.id, "resume-test");
    assert_eq!(current.messages.len(), 1);
}

#[test]
fn test_load_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let result = mgr.load_session("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_legacy_json_format() {
    // Test loading from legacy format (messages in JSON, no JSONL)
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "legacy-test".to_string();
    session.messages.push(make_msg(Role::User, "old format"));

    // Write as legacy format (all in JSON, no JSONL)
    let json_path = tmp.path().join("legacy-test.json");
    let content = serde_json::to_string_pretty(&session).unwrap();
    std::fs::write(&json_path, content).unwrap();

    let loaded = mgr.load_session("legacy-test").unwrap();
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.messages[0].content, "old format");
}

#[test]
fn test_set_get_metadata() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    mgr.create_session();

    // No metadata set yet
    assert!(mgr.get_metadata("mode").is_none());

    // Set and get
    mgr.set_metadata("mode", "PLAN");
    assert_eq!(mgr.get_metadata("mode").as_deref(), Some("PLAN"));

    mgr.set_metadata("thinking_level", "High");
    assert_eq!(mgr.get_metadata("thinking_level").as_deref(), Some("High"));

    mgr.set_metadata("autonomy_level", "Auto");
    assert_eq!(mgr.get_metadata("autonomy_level").as_deref(), Some("Auto"));
}

#[test]
fn test_metadata_persists_across_save_load() {
    let tmp = TempDir::new().unwrap();
    let mut mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    mgr.create_session();

    let session_id = mgr.current_session().unwrap().id.clone();

    mgr.set_metadata("mode", "PLAN");
    mgr.set_metadata("thinking_level", "High");
    mgr.set_metadata("autonomy_level", "Manual");
    mgr.save_current().unwrap();

    // Load in a fresh manager
    let mgr2 = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let loaded = mgr2.load_session(&session_id).unwrap();

    assert_eq!(
        loaded.metadata.get("mode").and_then(|v| v.as_str()),
        Some("PLAN")
    );
    assert_eq!(
        loaded
            .metadata
            .get("thinking_level")
            .and_then(|v| v.as_str()),
        Some("High")
    );
    assert_eq!(
        loaded
            .metadata
            .get("autonomy_level")
            .and_then(|v| v.as_str()),
        Some("Manual")
    );
}

// --- Session forking tests ---

#[test]
fn test_fork_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "parent-sess".to_string();
    session.working_directory = Some("/tmp/project".to_string());
    session
        .messages
        .push(make_msg(Role::User, "first question"));
    session
        .messages
        .push(make_msg(Role::Assistant, "first answer"));
    session
        .messages
        .push(make_msg(Role::User, "second question"));
    session
        .messages
        .push(make_msg(Role::Assistant, "second answer"));
    mgr.save_session(&session).unwrap();

    let forked = mgr.fork_session("parent-sess", Some(2)).unwrap();
    assert_ne!(forked.id, "parent-sess");
    assert_eq!(forked.parent_id.as_deref(), Some("parent-sess"));
    assert_eq!(forked.messages.len(), 2);
    assert_eq!(forked.messages[0].content, "first question");
    assert_eq!(forked.messages[1].content, "first answer");
    assert_eq!(forked.working_directory.as_deref(), Some("/tmp/project"));

    // Verify it was persisted
    let loaded = mgr.load_session(&forked.id).unwrap();
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.parent_id.as_deref(), Some("parent-sess"));
}

#[test]
fn test_fork_session_at_zero() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "fork-zero".to_string();
    session.messages.push(make_msg(Role::User, "hello"));
    mgr.save_session(&session).unwrap();

    let forked = mgr.fork_session("fork-zero", Some(0)).unwrap();
    assert!(forked.messages.is_empty());
    assert_eq!(forked.parent_id.as_deref(), Some("fork-zero"));
}

#[test]
fn test_fork_session_out_of_bounds() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "fork-oob".to_string();
    session.messages.push(make_msg(Role::User, "hello"));
    mgr.save_session(&session).unwrap();

    let result = mgr.fork_session("fork-oob", Some(5));
    assert!(result.is_err());
}

#[test]
fn test_fork_nonexistent_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    let result = mgr.fork_session("no-such-session", Some(0));
    assert!(result.is_err());
}

#[test]
fn test_fork_generates_title() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "title-src".to_string();
    session
        .messages
        .push(make_msg(Role::User, "Implement the new auth flow"));
    session
        .messages
        .push(make_msg(Role::Assistant, "Sure, I will help"));
    mgr.save_session(&session).unwrap();

    let forked = mgr.fork_session("title-src", Some(2)).unwrap();
    let title = forked
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap();
    // Fork inherits parent title with "(fork #1)" suffix
    assert_eq!(title, "Implement the new auth flow (fork #1)");
}

// --- Title generation tests ---

#[test]
fn test_generate_title_short_message() {
    let msgs = vec![make_msg(Role::User, "Fix the login bug")];
    assert_eq!(
        generate_title_from_messages(&msgs),
        Some("Fix the login bug".to_string())
    );
}

#[test]
fn test_generate_title_long_message() {
    let msgs = vec![make_msg(
        Role::User,
        "Please help me refactor the authentication module to use OAuth2 instead of the custom token system we built",
    )];
    let title = generate_title_from_messages(&msgs).unwrap();
    assert!(title.len() <= 63); // 60 + "..."
    assert!(title.ends_with("..."));
}

#[test]
fn test_generate_title_no_user_messages() {
    let msgs = vec![make_msg(Role::Assistant, "Hello")];
    assert!(generate_title_from_messages(&msgs).is_none());
}

#[test]
fn test_generate_title_empty_messages() {
    let msgs: Vec<ChatMessage> = vec![];
    assert!(generate_title_from_messages(&msgs).is_none());
}

#[test]
fn test_generate_title_empty_content() {
    let msgs = vec![make_msg(Role::User, "   ")];
    assert!(generate_title_from_messages(&msgs).is_none());
}

#[test]
fn test_generate_title_exactly_60_chars() {
    // Exactly 60 chars, no truncation needed
    let text = "a]".repeat(30); // 60 chars
    let msgs = vec![make_msg(Role::User, &text)];
    let title = generate_title_from_messages(&msgs).unwrap();
    assert_eq!(title, text);
}

// --- Archiving tests ---

#[test]
fn test_archive_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "archive-test".to_string();
    session.messages.push(make_msg(Role::User, "hello"));
    mgr.save_session(&session).unwrap();

    mgr.archive_session("archive-test").unwrap();

    let loaded = mgr.load_session("archive-test").unwrap();
    assert!(loaded.is_archived());
    assert!(loaded.time_archived.is_some());
}

#[test]
fn test_unarchive_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "unarchive-test".to_string();
    mgr.save_session(&session).unwrap();

    mgr.archive_session("unarchive-test").unwrap();
    mgr.unarchive_session("unarchive-test").unwrap();

    let loaded = mgr.load_session("unarchive-test").unwrap();
    assert!(!loaded.is_archived());
}

#[test]
fn test_list_sessions_excludes_archived() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut s1 = Session::new();
    s1.id = "active-sess".to_string();
    mgr.save_session(&s1).unwrap();

    let mut s2 = Session::new();
    s2.id = "archived-sess".to_string();
    mgr.save_session(&s2).unwrap();
    mgr.archive_session("archived-sess").unwrap();

    // Default listing excludes archived
    let active = mgr.list_sessions(false);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "active-sess");

    // Include archived
    let all = mgr.list_sessions(true);
    assert_eq!(all.len(), 2);
}

#[test]
fn test_archive_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    assert!(mgr.archive_session("nope").is_err());
}

// --- Fork with None (copy all) tests ---

#[test]
fn test_fork_session_none_copies_all() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "fork-all".to_string();
    session.messages.push(make_msg(Role::User, "msg1"));
    session.messages.push(make_msg(Role::Assistant, "msg2"));
    session.messages.push(make_msg(Role::User, "msg3"));
    mgr.save_session(&session).unwrap();

    let forked = mgr.fork_session("fork-all", None).unwrap();
    assert_eq!(forked.messages.len(), 3);
    assert_eq!(forked.parent_id.as_deref(), Some("fork-all"));
}

// --- Session reverting tests ---

#[test]
fn test_revert_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "revert-test".to_string();
    session.messages.push(make_msg(Role::User, "step 0"));
    session.messages.push(make_msg(Role::Assistant, "step 1"));
    session.messages.push(make_msg(Role::User, "step 2"));
    session.messages.push(make_msg(Role::Assistant, "step 3"));
    mgr.save_session(&session).unwrap();

    mgr.revert_session("revert-test", 2).unwrap();

    let loaded = mgr.load_session("revert-test").unwrap();
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].content, "step 0");
    assert_eq!(loaded.messages[1].content, "step 1");
}

#[test]
fn test_revert_session_to_zero() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "revert-zero".to_string();
    session.messages.push(make_msg(Role::User, "hello"));
    mgr.save_session(&session).unwrap();

    mgr.revert_session("revert-zero", 0).unwrap();

    let loaded = mgr.load_session("revert-zero").unwrap();
    assert!(loaded.messages.is_empty());
}

#[test]
fn test_revert_session_out_of_bounds() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "revert-oob".to_string();
    session.messages.push(make_msg(Role::User, "hello"));
    mgr.save_session(&session).unwrap();

    let result = mgr.revert_session("revert-oob", 10);
    assert!(result.is_err());
}

#[test]
fn test_revert_nonexistent_session() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();
    assert!(mgr.revert_session("no-such", 0).is_err());
}

// --- Cross-session search tests ---

#[test]
fn test_search_sessions() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut s1 = Session::new();
    s1.id = "search-1".to_string();
    s1.messages.push(make_msg(Role::User, "Fix the login bug"));
    s1.messages
        .push(make_msg(Role::Assistant, "I will fix that"));
    mgr.save_session(&s1).unwrap();

    let mut s2 = Session::new();
    s2.id = "search-2".to_string();
    s2.messages.push(make_msg(Role::User, "Add a new feature"));
    mgr.save_session(&s2).unwrap();

    let mut s3 = Session::new();
    s3.id = "search-3".to_string();
    s3.messages
        .push(make_msg(Role::User, "Another login issue"));
    mgr.save_session(&s3).unwrap();

    let results = mgr.search_sessions("login");
    assert_eq!(results.len(), 2);

    // Check that both sessions with "login" are found
    let ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"search-1"));
    assert!(ids.contains(&"search-3"));
}

#[test]
fn test_search_sessions_case_insensitive() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut s1 = Session::new();
    s1.id = "case-test".to_string();
    s1.messages.push(make_msg(Role::User, "Fix the LOGIN bug"));
    mgr.save_session(&s1).unwrap();

    let results = mgr.search_sessions("login");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "case-test");
}

#[test]
fn test_search_sessions_returns_indices() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut s1 = Session::new();
    s1.id = "idx-test".to_string();
    s1.messages.push(make_msg(Role::User, "first message"));
    s1.messages
        .push(make_msg(Role::Assistant, "target keyword here"));
    s1.messages
        .push(make_msg(Role::User, "another target message"));
    mgr.save_session(&s1).unwrap();

    let results = mgr.search_sessions("target");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, vec![1, 2]);
}

#[test]
fn test_search_sessions_no_matches() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut s1 = Session::new();
    s1.id = "no-match".to_string();
    s1.messages.push(make_msg(Role::User, "hello world"));
    mgr.save_session(&s1).unwrap();

    let results = mgr.search_sessions("nonexistent");
    assert!(results.is_empty());
}

// --- get_forked_title tests ---

#[test]
fn test_forked_title_first_fork() {
    assert_eq!(get_forked_title("My Session"), "My Session (fork #1)");
}

#[test]
fn test_forked_title_increment() {
    assert_eq!(
        get_forked_title("My Session (fork #1)"),
        "My Session (fork #2)"
    );
}

#[test]
fn test_forked_title_high_number() {
    assert_eq!(
        get_forked_title("Debug auth (fork #99)"),
        "Debug auth (fork #100)"
    );
}

#[test]
fn test_forked_title_preserves_base() {
    let title = get_forked_title("Fix (parens) in title");
    assert_eq!(title, "Fix (parens) in title (fork #1)");
}

#[test]
fn test_forked_title_empty() {
    assert_eq!(get_forked_title(""), " (fork #1)");
}

#[test]
fn test_fork_session_uses_parent_title() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "fork-title-parent".to_string();
    session
        .metadata
        .insert("title".to_string(), serde_json::json!("Auth refactor"));
    session
        .messages
        .push(make_msg(Role::User, "Some unrelated first message"));
    mgr.save_session(&session).unwrap();

    let forked = mgr.fork_session("fork-title-parent", None).unwrap();
    let title = forked
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap();
    // Should use parent's explicit title, not re-generate from messages
    assert_eq!(title, "Auth refactor (fork #1)");
}

#[test]
fn test_fork_double_fork_increments() {
    let tmp = TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path().to_path_buf()).unwrap();

    let mut session = Session::new();
    session.id = "double-fork-src".to_string();
    session
        .metadata
        .insert("title".to_string(), serde_json::json!("My task (fork #2)"));
    session.messages.push(make_msg(Role::User, "hello"));
    mgr.save_session(&session).unwrap();

    let forked = mgr.fork_session("double-fork-src", None).unwrap();
    let title = forked
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(title, "My task (fork #3)");
}
