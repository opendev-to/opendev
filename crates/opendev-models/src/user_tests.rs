use super::*;

#[test]
fn test_user_new() {
    let user = User::new("alice".to_string(), "hashed_pw".to_string());
    assert_eq!(user.username, "alice");
    assert_eq!(user.role, "user");
    assert!(user.email.is_none());
}

#[test]
fn test_user_roundtrip() {
    let user = User::new("bob".to_string(), "hash123".to_string());
    let json = serde_json::to_string(&user).unwrap();
    let deserialized: User = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.username, "bob");
    assert_eq!(deserialized.id, user.id);
}

#[test]
fn test_user_touch() {
    let mut user = User::new("carol".to_string(), "hash".to_string());
    let before = user.updated_at;
    std::thread::sleep(std::time::Duration::from_millis(10));
    user.touch();
    assert!(user.updated_at >= before);
}
