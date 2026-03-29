//! Session listing, finding, and deletion.

use std::path::{Path, PathBuf};

use opendev_models::SessionMetadata;

use crate::index::{SESSIONS_INDEX_FILE_NAME, SessionIndex};

/// Session listing operations.
///
/// Provides methods for listing, searching, and deleting sessions.
pub struct SessionListing {
    session_dir: PathBuf,
    index: SessionIndex,
}

impl SessionListing {
    pub fn new(session_dir: PathBuf) -> Self {
        let index = SessionIndex::new(session_dir.clone());
        Self { session_dir, index }
    }

    /// List saved sessions, optionally filtered by owner.
    pub fn list_sessions(
        &self,
        owner_id: Option<&str>,
        include_archived: bool,
    ) -> Vec<SessionMetadata> {
        let mut sessions = if let Some(index) = self.index.read_index() {
            let entries = if include_archived {
                index.entries
            } else {
                index
                    .entries
                    .into_iter()
                    .filter(|e| e.time_archived.is_none())
                    .collect()
            };
            entries
                .iter()
                .map(SessionIndex::entry_to_metadata)
                .collect()
        } else {
            // Index missing/corrupted; return empty for now
            // (rebuild_index would require loading session files)
            Vec::new()
        };

        if let Some(owner) = owner_id {
            sessions.retain(|s| s.owner_id.as_deref() == Some(owner));
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    /// Find the most recently updated session.
    pub fn find_latest_session(&self) -> Option<SessionMetadata> {
        let sessions = self.list_sessions(None, false);
        sessions.into_iter().next()
    }

    /// Find active session for a channel+user combination.
    pub fn find_session_by_channel_user(
        &self,
        channel: &str,
        user_id: &str,
        thread_id: Option<&str>,
    ) -> Option<SessionMetadata> {
        let sessions = self.list_sessions(None, false);
        sessions.into_iter().find(|s| {
            s.channel == channel
                && s.channel_user_id == user_id
                && (thread_id.is_none() || s.thread_id.as_deref() == thread_id)
        })
    }

    /// Delete a session and its associated files.
    pub fn delete_session(&self, session_id: &str) -> std::io::Result<()> {
        let session_file = self.session_dir.join(format!("{session_id}.json"));

        // Delete .json metadata
        if session_file.exists() {
            std::fs::remove_file(&session_file)?;
        }

        // Delete .jsonl transcript
        let jsonl_file = self.session_dir.join(format!("{session_id}.jsonl"));
        if jsonl_file.exists() {
            std::fs::remove_file(&jsonl_file)?;
        }

        // Delete .debug log
        let debug_file = self.session_dir.join(format!("{session_id}.debug"));
        if debug_file.exists() {
            std::fs::remove_file(&debug_file)?;
        }

        // Remove from index
        self.index.remove_entry(session_id)?;

        Ok(())
    }

    /// List sessions across all project workspaces, merged and sorted by recency.
    pub fn list_all_sessions(projects_dir: &Path) -> Vec<SessionMetadata> {
        let mut all = Vec::new();
        for workspace in Self::list_user_workspaces(projects_dir) {
            let listing = SessionListing::new(projects_dir.join(&workspace));
            all.extend(listing.list_sessions(None, false));
        }
        all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        all
    }

    /// List all workspaces that have OpenDev sessions.
    pub fn list_user_workspaces(projects_dir: &Path) -> Vec<String> {
        if !projects_dir.exists() {
            return Vec::new();
        }

        let mut workspaces = Vec::new();
        if let Ok(entries) = std::fs::read_dir(projects_dir) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                // Check if directory has session files
                if let Ok(files) = std::fs::read_dir(entry.path()) {
                    let has_sessions = files.flatten().any(|f| {
                        f.path().extension().map(|e| e == "json").unwrap_or(false)
                            && f.file_name() != SESSIONS_INDEX_FILE_NAME
                    });
                    if has_sessions {
                        workspaces.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }

        workspaces.sort();
        workspaces
    }
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
