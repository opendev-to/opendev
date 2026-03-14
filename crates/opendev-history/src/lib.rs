//! Session history management for OpenDev.
//!
//! This crate provides:
//! - **SessionManager**: JSON read/write to ~/.opendev/sessions/
//! - **SessionIndex**: Fast metadata lookups via cached index
//! - **SessionListing**: Session listing and search
//! - **FileLock**: Cross-platform exclusive file locking
//! - **SnapshotManager**: Shadow git snapshots for per-step undo

pub mod export;
pub mod fair_rwlock;
pub mod file_locks;
pub mod index;
pub mod listing;
pub mod session_manager;
pub mod sharing;
pub mod snapshot;
pub mod sqlite_store;

pub use export::export_markdown;
pub use fair_rwlock::FairRwLock;
pub use file_locks::FileLock;
pub use index::SessionIndex;
pub use listing::SessionListing;
pub use session_manager::{SessionManager, generate_title_from_messages};
pub use sharing::share_session;
pub use snapshot::SnapshotManager;
pub use sqlite_store::SqliteSessionStore;
