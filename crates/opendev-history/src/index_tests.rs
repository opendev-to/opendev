use super::*;
use tempfile::TempDir;

fn make_test_session(id: &str) -> Session {
    let mut session = Session::new();
    session.id = id.to_string();
    session
}

#[test]
fn test_read_nonexistent_index() {
    let tmp = TempDir::new().unwrap();
    let index = SessionIndex::new(tmp.path().to_path_buf());
    assert!(index.read_index().is_none());
}

#[test]
fn test_write_and_read_index() {
    let tmp = TempDir::new().unwrap();
    let index = SessionIndex::new(tmp.path().to_path_buf());

    let session = make_test_session("test-123");
    let entry = SessionIndex::session_to_entry(&session);
    index.write_index(&[entry.clone()]).unwrap();

    let read_back = index.read_index().unwrap();
    assert_eq!(read_back.version, INDEX_VERSION);
    assert_eq!(read_back.entries.len(), 1);
    assert_eq!(read_back.entries[0].session_id, "test-123");
}

#[test]
fn test_upsert_entry() {
    let tmp = TempDir::new().unwrap();
    let index = SessionIndex::new(tmp.path().to_path_buf());

    let session1 = make_test_session("s1");
    let session2 = make_test_session("s2");

    index.upsert_entry(&session1).unwrap();
    index.upsert_entry(&session2).unwrap();

    let read_back = index.read_index().unwrap();
    assert_eq!(read_back.entries.len(), 2);

    // Update s1
    index.upsert_entry(&session1).unwrap();
    let read_back = index.read_index().unwrap();
    assert_eq!(read_back.entries.len(), 2); // No duplicate
}

#[test]
fn test_remove_entry() {
    let tmp = TempDir::new().unwrap();
    let index = SessionIndex::new(tmp.path().to_path_buf());

    let session1 = make_test_session("s1");
    let session2 = make_test_session("s2");
    index.upsert_entry(&session1).unwrap();
    index.upsert_entry(&session2).unwrap();

    index.remove_entry("s1").unwrap();

    let read_back = index.read_index().unwrap();
    assert_eq!(read_back.entries.len(), 1);
    assert_eq!(read_back.entries[0].session_id, "s2");
}

#[test]
fn test_entry_to_metadata_roundtrip() {
    let session = make_test_session("rt-test");
    let entry = SessionIndex::session_to_entry(&session);
    let metadata = SessionIndex::entry_to_metadata(&entry);
    assert_eq!(metadata.id, "rt-test");
    assert_eq!(metadata.channel, "cli");
}

#[test]
fn test_invalid_index_version() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join(SESSIONS_INDEX_FILE_NAME);
    let bad_index = serde_json::json!({"version": 999, "entries": []});
    std::fs::write(&index_path, serde_json::to_string(&bad_index).unwrap()).unwrap();

    let index = SessionIndex::new(tmp.path().to_path_buf());
    assert!(index.read_index().is_none());
}
