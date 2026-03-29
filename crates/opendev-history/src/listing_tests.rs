use super::*;
use crate::index::SessionIndex;
use opendev_models::Session;
use tempfile::TempDir;

fn setup_with_sessions(count: usize) -> (TempDir, SessionListing) {
    let tmp = TempDir::new().unwrap();
    let listing = SessionListing::new(tmp.path().to_path_buf());
    let index = SessionIndex::new(tmp.path().to_path_buf());

    for i in 0..count {
        let mut session = Session::new();
        session.id = format!("session-{i}");
        index.upsert_entry(&session).unwrap();
    }

    (tmp, listing)
}

#[test]
fn test_list_sessions_empty() {
    let tmp = TempDir::new().unwrap();
    let listing = SessionListing::new(tmp.path().to_path_buf());
    let sessions = listing.list_sessions(None, false);
    assert!(sessions.is_empty());
}

#[test]
fn test_list_sessions() {
    let (_tmp, listing) = setup_with_sessions(3);
    let sessions = listing.list_sessions(None, false);
    assert_eq!(sessions.len(), 3);
}

#[test]
fn test_find_latest_session() {
    let (_tmp, listing) = setup_with_sessions(3);
    let latest = listing.find_latest_session();
    assert!(latest.is_some());
}

#[test]
fn test_delete_session() {
    let (tmp, listing) = setup_with_sessions(2);

    // Create a fake session file
    std::fs::write(tmp.path().join("session-0.json"), "{}").unwrap();

    listing.delete_session("session-0").unwrap();

    let sessions = listing.list_sessions(None, false);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "session-1");
}

#[test]
fn test_find_by_channel_user() {
    let tmp = TempDir::new().unwrap();
    let listing = SessionListing::new(tmp.path().to_path_buf());
    let index = SessionIndex::new(tmp.path().to_path_buf());

    let mut session = Session::new();
    session.id = "tg-session".to_string();
    session.channel = "telegram".to_string();
    session.channel_user_id = "user123".to_string();
    index.upsert_entry(&session).unwrap();

    let found = listing.find_session_by_channel_user("telegram", "user123", None);
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, "tg-session");

    let not_found = listing.find_session_by_channel_user("whatsapp", "user123", None);
    assert!(not_found.is_none());
}
