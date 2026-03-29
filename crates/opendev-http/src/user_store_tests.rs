use super::*;

#[test]
fn test_create_and_get_user() {
    let dir = tempfile::tempdir().unwrap();
    let store = UserStore::new(dir.path().to_path_buf()).unwrap();

    let user = store
        .create_user("alice", "hashed_pw", Some("alice@example.com"))
        .unwrap();
    assert_eq!(user.username, "alice");
    assert_eq!(user.email.as_deref(), Some("alice@example.com"));
    assert_eq!(user.role, "user");

    let found = store.get_by_username("alice").unwrap();
    assert_eq!(found.id, user.id);
}

#[test]
fn test_create_duplicate_user_fails() {
    let dir = tempfile::tempdir().unwrap();
    let store = UserStore::new(dir.path().to_path_buf()).unwrap();

    store.create_user("bob", "hash1", None).unwrap();
    let result = store.create_user("bob", "hash2", None);
    assert!(result.is_err());
}

#[test]
fn test_get_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let store = UserStore::new(dir.path().to_path_buf()).unwrap();

    let user = store.create_user("carol", "hash", None).unwrap();
    let found = store.get_by_id(user.id).unwrap();
    assert_eq!(found.username, "carol");

    assert!(store.get_by_id(Uuid::new_v4()).is_none());
}

#[test]
fn test_update_user() {
    let dir = tempfile::tempdir().unwrap();
    let store = UserStore::new(dir.path().to_path_buf()).unwrap();

    let mut user = store.create_user("dave", "hash", None).unwrap();
    user.email = Some("dave@example.com".to_string());
    user.role = "admin".to_string();
    store.update_user(user.clone()).unwrap();

    let found = store.get_by_username("dave").unwrap();
    assert_eq!(found.email.as_deref(), Some("dave@example.com"));
    assert_eq!(found.role, "admin");
}

#[test]
fn test_delete_user() {
    let dir = tempfile::tempdir().unwrap();
    let store = UserStore::new(dir.path().to_path_buf()).unwrap();

    store.create_user("eve", "hash", None).unwrap();
    assert!(store.delete_user("eve").unwrap());
    assert!(store.get_by_username("eve").is_none());
    assert!(!store.delete_user("eve").unwrap());
}

#[test]
fn test_list_and_count() {
    let dir = tempfile::tempdir().unwrap();
    let store = UserStore::new(dir.path().to_path_buf()).unwrap();

    assert_eq!(store.count(), 0);
    store.create_user("u1", "h1", None).unwrap();
    store.create_user("u2", "h2", None).unwrap();
    assert_eq!(store.count(), 2);

    let mut names = store.list_usernames();
    names.sort();
    assert_eq!(names, vec!["u1", "u2"]);
}

#[test]
fn test_persistence_across_instances() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();

    // Create a user with first instance
    {
        let store = UserStore::new(path.clone()).unwrap();
        store.create_user("frank", "hash", None).unwrap();
    }

    // Read with a new instance
    {
        let store = UserStore::new(path).unwrap();
        let found = store.get_by_username("frank");
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "frank");
    }
}

#[test]
fn test_empty_dir_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("nested").join("dir");
    let store = UserStore::new(sub.clone()).unwrap();
    assert!(sub.join("users.json").exists());
    assert_eq!(store.count(), 0);
}

#[test]
fn test_get_nonexistent_user() {
    let dir = tempfile::tempdir().unwrap();
    let store = UserStore::new(dir.path().to_path_buf()).unwrap();
    assert!(store.get_by_username("nobody").is_none());
}
