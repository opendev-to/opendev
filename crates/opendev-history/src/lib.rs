//! Session history management for OpenDev.
//!
//! This crate provides:
//! - **SessionManager**: JSON read/write to ~/.opendev/sessions/
//! - **SessionIndex**: Fast metadata lookups via cached index
//! - **SessionListing**: Session listing and search
//! - **FileLock**: Cross-platform exclusive file locking
//! - **SnapshotManager**: Shadow git snapshots for per-step undo

pub mod event_store;
pub mod export;
pub mod fair_rwlock;
pub mod file_checkpoint;
pub mod file_locks;
pub mod index;
pub mod listing;
pub mod message_convert;
pub mod projector;
pub mod session_manager;
pub mod sharing;
pub mod sidechain;
pub mod snapshot;
pub mod sqlite_store;
pub mod topic_detector;

pub use export::export_markdown;
pub use fair_rwlock::FairRwLock;
pub use file_checkpoint::FileCheckpointManager;
pub use file_locks::FileLock;
pub use index::SessionIndex;
pub use listing::SessionListing;
pub use session_manager::{SessionManager, generate_title_from_messages, get_forked_title};
pub use sharing::share_session;
pub use sidechain::{EntryKind, SidechainReader, SidechainWriter, TranscriptEntry};
pub use snapshot::{DiffStatus, DiffSummary, FileDiff, FileDiffStat, SnapshotManager};
pub use sqlite_store::SqliteSessionStore;
pub use topic_detector::TopicDetector;
