use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::warn;

const MAX_INBOX_SIZE: usize = 1000;

/// A single message in the agent's mailbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MailboxMessage {
    pub id: String,
    pub from: String,
    pub content: String,
    pub timestamp_ms: u64,
    pub read: bool,
    #[serde(default)]
    pub msg_type: MessageType,
}

/// Message type discriminant for structured communication.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    #[default]
    Text,
    ShutdownRequest,
    ShutdownResponse,
    Idle,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// File-based inbox for an agent.
///
/// Thread-safe via file locking (fs2). Each operation acquires an
/// exclusive lock, performs the read-modify-write, and releases.
pub struct Mailbox {
    inbox_path: PathBuf,
}

impl Mailbox {
    /// Create a mailbox for the given agent in the given team directory.
    pub fn new(team_dir: &Path, agent_name: &str) -> Self {
        let inbox_path = team_dir.join("inboxes").join(format!("{agent_name}.json"));
        Self { inbox_path }
    }

    /// Ensure the inbox file and parent directories exist.
    fn ensure_file(&self) -> io::Result<()> {
        if let Some(parent) = self.inbox_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_path = self.inbox_path.with_extension("lock");
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        let mut rw_lock = fd_lock::RwLock::new(lock_file);
        let _guard = rw_lock
            .write()
            .map_err(|e| io::Error::other(format!("Could not acquire mailbox lock: {e}")))?;

        if !self.inbox_path.exists() {
            fs::write(&self.inbox_path, "[]")?;
        }

        Ok(())
    }

    /// Send a message to this inbox.
    pub fn send(&self, msg: MailboxMessage) -> io::Result<()> {
        self.ensure_file()?;
        let mut msg_opt = Some(msg);
        self.with_locked_inbox(|messages| {
            if let Some(m) = msg_opt.take() {
                messages.push(m);
            }
            // Trim old read messages if over limit
            if messages.len() > MAX_INBOX_SIZE {
                let excess = messages.len() - MAX_INBOX_SIZE;
                let mut removed = 0;
                messages.retain(|m| {
                    if removed >= excess {
                        return true;
                    }
                    if m.read {
                        removed += 1;
                        return false;
                    }
                    true
                });
            }
        })
    }

    /// Receive all unread messages, marking them as read.
    pub fn receive(&self) -> io::Result<Vec<MailboxMessage>> {
        self.ensure_file()?;
        let mut received = Vec::new();
        self.with_locked_inbox(|messages| {
            for msg in messages.iter_mut() {
                if !msg.read {
                    msg.read = true;
                    received.push(msg.clone());
                }
            }
        })?;
        Ok(received)
    }

    /// Peek at unread messages without marking them as read.
    pub fn peek(&self) -> io::Result<Vec<MailboxMessage>> {
        self.ensure_file()?;
        let mut result = Vec::new();
        self.with_locked_inbox(|messages| {
            result = messages.iter().filter(|m| !m.read).cloned().collect();
        })?;
        Ok(result)
    }

    /// Poll for new messages with a timeout.
    pub async fn poll(&self, timeout: Duration) -> io::Result<Option<Vec<MailboxMessage>>> {
        let poll_interval = Duration::from_millis(500);
        let start = std::time::Instant::now();

        loop {
            let messages = self.peek()?;
            if !messages.is_empty() {
                return Ok(Some(self.receive()?));
            }
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Path to the inbox file.
    pub fn path(&self) -> &Path {
        &self.inbox_path
    }

    // -- Internal helpers --

    fn read_messages(&self) -> io::Result<Vec<MailboxMessage>> {
        let content = fs::read_to_string(&self.inbox_path)?;
        match serde_json::from_str::<Vec<MailboxMessage>>(&content) {
            Ok(messages) => Ok(messages),
            Err(e) => {
                // Corrupt inbox — rename and start fresh
                warn!(
                    path = %self.inbox_path.display(),
                    error = %e,
                    "Corrupt mailbox inbox, resetting"
                );
                let backup = self
                    .inbox_path
                    .with_extension(format!("corrupt.{}", now_ms()));
                let _ = fs::rename(&self.inbox_path, &backup);
                fs::write(&self.inbox_path, "[]")?;
                Ok(Vec::new())
            }
        }
    }

    fn write_messages(&self, messages: &[MailboxMessage]) -> io::Result<()> {
        let json = serde_json::to_string_pretty(messages).map_err(io::Error::other)?;

        let tmp_path = self
            .inbox_path
            .with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
        fs::write(&tmp_path, &json)?;
        fs::rename(&tmp_path, &self.inbox_path)?;
        Ok(())
    }

    /// Acquire file lock, read inbox, apply mutation, write back, release lock.
    fn with_locked_inbox<F>(&self, mut f: F) -> io::Result<()>
    where
        F: FnMut(&mut Vec<MailboxMessage>),
    {
        // Use a separate lock file for exclusive access
        let lock_path = self.inbox_path.with_extension("lock");
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;

        // Blocking lock acquisition (fd-lock)
        let mut rw_lock = fd_lock::RwLock::new(lock_file);
        let _guard = rw_lock
            .write()
            .map_err(|e| io::Error::other(format!("Could not acquire mailbox lock: {e}")))?;

        // Read, mutate, write back under lock
        let mut messages = self.read_messages()?;
        f(&mut messages);
        self.write_messages(&messages)?;

        Ok(())
    }
}

impl std::fmt::Debug for Mailbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mailbox")
            .field("path", &self.inbox_path)
            .finish()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
