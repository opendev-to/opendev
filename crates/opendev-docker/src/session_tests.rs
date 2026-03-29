use super::*;

#[test]
fn test_strip_ansi() {
    assert_eq!(strip_ansi("\x1B[31mred\x1B[0m"), "red");
    assert_eq!(strip_ansi("hello\r\nworld"), "hello\nworld");
    assert_eq!(strip_ansi("plain text"), "plain text");
}

#[test]
fn test_session_new() {
    let s = DockerSession::new("abc123", "default");
    assert_eq!(s.container_id(), "abc123");
    assert_eq!(s.name(), "default");
    assert!(s.working_dir.is_none());
}

#[test]
fn test_session_set_working_dir() {
    let mut s = DockerSession::new("abc123", "default");
    s.set_working_dir("/workspace");
    assert_eq!(s.working_dir.as_deref(), Some("/workspace"));
}
