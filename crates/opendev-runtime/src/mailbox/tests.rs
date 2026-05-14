use tempfile::TempDir;

use super::*;

fn temp_team_dir() -> TempDir {
    TempDir::new().unwrap()
}

fn make_msg(from: &str, content: &str) -> MailboxMessage {
    MailboxMessage {
        id: uuid::Uuid::new_v4().to_string(),
        from: from.to_string(),
        content: content.to_string(),
        timestamp_ms: now_ms(),
        read: false,
        msg_type: MessageType::Text,
    }
}

#[test]
fn test_send_and_receive() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    mailbox.send(make_msg("leader", "hello")).unwrap();
    mailbox.send(make_msg("leader", "world")).unwrap();

    let msgs = mailbox.receive().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].content, "hello");
    assert_eq!(msgs[1].content, "world");
}

#[test]
fn test_receive_marks_read() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    mailbox.send(make_msg("leader", "msg1")).unwrap();
    let _ = mailbox.receive().unwrap();

    // Second receive returns empty (already read)
    let msgs = mailbox.receive().unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_peek_does_not_mark_read() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    mailbox.send(make_msg("leader", "msg1")).unwrap();

    let peeked = mailbox.peek().unwrap();
    assert_eq!(peeked.len(), 1);

    // Still unread after peek
    let received = mailbox.receive().unwrap();
    assert_eq!(received.len(), 1);
}

#[test]
fn test_empty_mailbox_returns_empty() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    mailbox.ensure_file().unwrap();
    let msgs = mailbox.receive().unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_send_creates_file_if_missing() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    assert!(!mailbox.path().exists());
    mailbox.send(make_msg("leader", "msg1")).unwrap();
    assert!(mailbox.path().exists());
}

#[test]
fn test_concurrent_writes() {
    use std::sync::Arc;
    use std::thread;

    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let td = team_dir.clone();
            thread::spawn(move || {
                let mailbox = Mailbox::new(&td, "agent-a");
                mailbox
                    .send(make_msg(&format!("thread-{i}"), &format!("msg from {i}")))
                    .unwrap();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let mailbox = Mailbox::new(&team_dir, "agent-a");
    let msgs = mailbox.receive().unwrap();
    assert_eq!(msgs.len(), 5);
}

#[test]
fn test_corrupt_inbox_recovery() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    // Create corrupt file
    mailbox.ensure_file().unwrap();
    fs::write(mailbox.path(), "{{not valid json}}").unwrap();

    // Should recover gracefully
    let msgs = mailbox.receive().unwrap();
    assert!(msgs.is_empty());

    // Should work normally after recovery
    mailbox.send(make_msg("leader", "after recovery")).unwrap();
    let msgs = mailbox.receive().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "after recovery");
}

#[test]
fn test_message_cap_trims_old_read() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    // Send MAX_INBOX_SIZE messages and read them all
    for i in 0..MAX_INBOX_SIZE {
        mailbox
            .send(make_msg("leader", &format!("msg {i}")))
            .unwrap();
    }
    let _ = mailbox.receive().unwrap(); // mark all as read

    // Send one more — should trim oldest read messages
    mailbox.send(make_msg("leader", "overflow")).unwrap();

    // Read the raw file to check total count
    let content = fs::read_to_string(mailbox.path()).unwrap();
    let all: Vec<MailboxMessage> = serde_json::from_str(&content).unwrap();
    assert!(all.len() <= MAX_INBOX_SIZE);
}

#[test]
fn test_message_types() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");

    let mut msg = make_msg("leader", "please stop");
    msg.msg_type = MessageType::ShutdownRequest;
    mailbox.send(msg).unwrap();

    let msgs = mailbox.receive().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].msg_type, MessageType::ShutdownRequest);
}

#[tokio::test]
async fn test_poll_with_timeout() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();
    let mailbox = Mailbox::new(&team_dir, "agent-a");
    mailbox.ensure_file().unwrap();

    // Poll with short timeout — should return None
    let result = mailbox.poll(Duration::from_millis(100)).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_poll_returns_on_new_message() {
    let dir = temp_team_dir();
    let team_dir = dir.path().canonicalize().unwrap();

    let td = team_dir.clone();
    // Send a message after 50ms in a background task
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mailbox = Mailbox::new(&td, "agent-a");
        mailbox.send(make_msg("leader", "delayed")).unwrap();
    });

    let mailbox = Mailbox::new(&team_dir, "agent-a");
    mailbox.ensure_file().unwrap();
    let result = mailbox.poll(Duration::from_secs(5)).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap()[0].content, "delayed");
}
