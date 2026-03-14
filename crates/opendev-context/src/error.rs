//! Unified error types for the context engineering crate.

/// Errors that can occur during context engineering operations.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// Token counting failed.
    #[error("token counting failed: {0}")]
    TokenCountingFailed(String),

    /// Context compaction failed.
    #[error("compaction failed: {0}")]
    CompactionFailed(String),

    /// Invalid message structure or ordering.
    #[error("invalid message: {0}")]
    InvalidMessage(String),

    /// JSON serialization/deserialization error.
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// I/O error (e.g., git worktree operations).
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Git command failed.
    #[error("git error: {0}")]
    GitError(String),
}
