//! Session history management for OpenDev.
//!
//! This crate provides:
//! - **SessionManager**: JSON read/write to ~/.opendev/sessions/
//! - **SessionIndex**: Fast metadata lookups via cached index
//! - **SessionListing**: Session listing and search
//! - **FileLock**: Cross-platform exclusive file locking
//! - **SnapshotManager**: Shadow git snapshots for per-step undo

pub mod file_locks;
pub mod index;
pub mod listing;
pub mod session_manager;
pub mod snapshot;

pub use file_locks::FileLock;
pub use index::SessionIndex;
pub use listing::SessionListing;
pub use session_manager::{SessionManager, generate_title_from_messages};
pub use snapshot::SnapshotManager;
