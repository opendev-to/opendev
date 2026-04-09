//! File-level checkpoint system for per-turn undo.
//!
//! Replaces the shadow git snapshot system with lightweight file copies.
//! Only files that tools actually modify are captured, keeping disk usage
//! proportional to edited files rather than entire workspace size.
//!
//! Storage layout:
//! ```text
//! ~/.opendev/file-history/{session_id}/
//!   manifest.json          # serialized turn metadata
//!   {path_hash}@v{N}       # before-content of edited files
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use similar::TextDiff;
use tracing::{debug, info, warn};

use crate::snapshot::FileDiffStat;

/// Maximum number of file snapshots before oldest turns are pruned.
const MAX_SNAPSHOTS: usize = 100;

/// Sessions older than this are cleaned up (30 days).
const RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

/// Manifest file name within a session checkpoint directory.
const MANIFEST_FILE: &str = "manifest.json";

/// Compute a stable, filesystem-safe hash for a file path.
fn path_hash(path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Metadata for a single file snapshot (before-edit copy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Absolute path of the original file.
    pub abs_path: String,
    /// Hash-based identifier for the file path.
    pub path_hash: String,
    /// Backup file name (e.g., "a1b2c3d4@v1"). None if the file didn't exist.
    pub backup_file: Option<String>,
    /// Version counter for this file path.
    pub version: u32,
}

/// A group of file snapshots captured during one user turn (query).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCheckpoint {
    /// Monotonic turn identifier.
    pub turn_id: u32,
    /// Files captured in this turn.
    pub files: Vec<FileSnapshot>,
    /// When this turn was checkpointed.
    pub timestamp: DateTime<Utc>,
}

/// Persisted manifest containing all turn checkpoints for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    session_id: String,
    turns: Vec<TurnCheckpoint>,
    next_turn_id: u32,
    next_version: HashMap<String, u32>,
}

/// Manages file-level checkpoints for per-turn undo.
///
/// Before each file-modifying tool executes, the middleware calls
/// `capture_file()` to save the current content. At the end of a turn,
/// `end_turn_with_stats()` computes diff statistics and persists the manifest.
pub struct FileCheckpointManager {
    session_id: String,
    working_dir: PathBuf,
    base_dir: PathBuf,
    turns: Vec<TurnCheckpoint>,
    current_turn: Option<TurnCheckpoint>,
    next_turn_id: u32,
    next_version: HashMap<String, u32>,
}

impl FileCheckpointManager {
    /// Create a new checkpoint manager for a session.
    pub fn new(session_id: &str, working_dir: &Path) -> Self {
        let base_dir = opendev_config::Paths::default()
            .data_dir()
            .join("file-history")
            .join(session_id);

        let mut mgr = Self {
            session_id: session_id.to_string(),
            working_dir: working_dir.to_path_buf(),
            base_dir,
            turns: Vec::new(),
            current_turn: None,
            next_turn_id: 0,
            next_version: HashMap::new(),
        };

        // Load existing manifest if resuming a session
        mgr.load_manifest();
        mgr
    }

    /// Start a new turn checkpoint (called at the beginning of each query).
    pub fn begin_turn(&mut self) {
        let turn = TurnCheckpoint {
            turn_id: self.next_turn_id,
            files: Vec::new(),
            timestamp: Utc::now(),
        };
        self.next_turn_id += 1;
        self.current_turn = Some(turn);
    }

    /// Capture a file's current content before it is modified.
    ///
    /// If the file doesn't exist (new file being created), records that fact
    /// so undo can delete the file. Deduplicates within a turn — if the same
    /// file is captured twice, only the first copy is kept.
    pub fn capture_file(&mut self, abs_path: &Path) -> Result<(), String> {
        let turn = self
            .current_turn
            .as_mut()
            .ok_or_else(|| "No active turn — call begin_turn() first".to_string())?;

        let path_str = abs_path.to_string_lossy().to_string();
        let hash = path_hash(&path_str);

        // Dedup: skip if already captured in this turn
        if turn.files.iter().any(|f| f.path_hash == hash) {
            debug!("File already captured this turn: {}", path_str);
            return Ok(());
        }

        // Ensure base directory exists
        if let Err(e) = std::fs::create_dir_all(&self.base_dir) {
            return Err(format!("Failed to create checkpoint dir: {e}"));
        }

        let version = self.next_version.entry(hash.clone()).or_insert(1);
        let current_version = *version;
        *version += 1;

        let backup_file = if abs_path.exists() {
            let backup_name = format!("{hash}@v{current_version}");
            let backup_path = self.base_dir.join(&backup_name);

            // Atomic copy: write to temp, then rename
            let tmp_path = self.base_dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
            std::fs::copy(abs_path, &tmp_path)
                .map_err(|e| format!("Failed to copy file for checkpoint: {e}"))?;
            std::fs::rename(&tmp_path, &backup_path)
                .map_err(|e| format!("Failed to finalize checkpoint: {e}"))?;

            debug!("Captured checkpoint: {} -> {}", path_str, backup_name);
            Some(backup_name)
        } else {
            debug!("File doesn't exist yet (will be created): {}", path_str);
            None
        };

        turn.files.push(FileSnapshot {
            abs_path: path_str,
            path_hash: hash,
            backup_file,
            version: current_version,
        });

        Ok(())
    }

    /// End the current turn and compute diff statistics.
    ///
    /// For each captured file, compares the before-snapshot with the current
    /// file on disk to produce line-level diff stats. Persists the manifest
    /// and enforces the snapshot cap.
    pub fn end_turn_with_stats(&mut self) -> Vec<FileDiffStat> {
        let turn = match self.current_turn.take() {
            Some(t) => t,
            None => return Vec::new(),
        };

        if turn.files.is_empty() {
            return Vec::new();
        }

        let mut stats = Vec::new();

        for snapshot in &turn.files {
            let before = snapshot
                .backup_file
                .as_ref()
                .and_then(|name| std::fs::read_to_string(self.base_dir.join(name)).ok())
                .unwrap_or_default();

            let after = std::fs::read_to_string(&snapshot.abs_path).unwrap_or_default();

            // Check for binary content (contains null bytes)
            let is_binary = before.as_bytes().contains(&0) || after.as_bytes().contains(&0);

            let (additions, deletions) = if is_binary {
                (0, 0)
            } else {
                let diff = TextDiff::from_lines(&before, &after);
                let mut adds = 0u64;
                let mut dels = 0u64;
                for change in diff.iter_all_changes() {
                    match change.tag() {
                        similar::ChangeTag::Insert => adds += 1,
                        similar::ChangeTag::Delete => dels += 1,
                        similar::ChangeTag::Equal => {}
                    }
                }
                (adds, dels)
            };

            // Compute relative path for display
            let file_path = Path::new(&snapshot.abs_path)
                .strip_prefix(&self.working_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| snapshot.abs_path.clone());

            if additions > 0 || deletions > 0 || snapshot.backup_file.is_none() {
                stats.push(FileDiffStat {
                    file_path,
                    additions,
                    deletions,
                    is_binary,
                });
            }
        }

        self.turns.push(turn);
        self.enforce_snapshot_cap();
        self.save_manifest();

        stats
    }

    /// Undo the most recent turn, restoring files to their pre-edit state.
    ///
    /// Returns a human-readable description of what was reverted, or None
    /// if there are no turns to undo.
    pub fn undo_last_turn(&mut self) -> Option<String> {
        let turn = self.turns.pop()?;

        if turn.files.is_empty() {
            return None;
        }

        let mut restored = Vec::new();

        for snapshot in &turn.files {
            let target_path = Path::new(&snapshot.abs_path);

            match &snapshot.backup_file {
                Some(backup_name) => {
                    // File existed before — restore from backup
                    let backup_path = self.base_dir.join(backup_name);
                    if backup_path.exists() {
                        match std::fs::copy(&backup_path, target_path) {
                            Ok(_) => {
                                restored.push(snapshot.abs_path.clone());
                                debug!("Restored: {}", snapshot.abs_path);
                            }
                            Err(e) => {
                                warn!("Failed to restore {}: {}", snapshot.abs_path, e);
                            }
                        }
                    }
                }
                None => {
                    // File didn't exist before — delete it
                    if target_path.exists() {
                        match std::fs::remove_file(target_path) {
                            Ok(_) => {
                                restored.push(snapshot.abs_path.clone());
                                debug!("Deleted (was new): {}", snapshot.abs_path);
                            }
                            Err(e) => {
                                warn!("Failed to delete {}: {}", snapshot.abs_path, e);
                            }
                        }
                    }
                }
            }
        }

        if restored.is_empty() {
            return None;
        }

        self.save_manifest();

        // Build description using relative paths
        let rel_paths: Vec<String> = restored
            .iter()
            .map(|p| {
                Path::new(p)
                    .strip_prefix(&self.working_dir)
                    .map(|r| r.to_string_lossy().to_string())
                    .unwrap_or_else(|_| p.clone())
            })
            .collect();

        let desc = if rel_paths.len() <= 5 {
            format!(
                "Reverted {} file(s) to previous state: {}",
                rel_paths.len(),
                rel_paths.join(", ")
            )
        } else {
            format!("Reverted {} file(s) to previous state", rel_paths.len())
        };

        info!("{}", desc);
        Some(desc)
    }

    /// Number of completed turns available for undo.
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }

    /// Remove checkpoint session directories older than the retention period.
    pub fn cleanup_old_sessions() {
        let history_base = opendev_config::Paths::default()
            .data_dir()
            .join("file-history");

        let entries = match std::fs::read_dir(&history_base) {
            Ok(e) => e,
            Err(_) => return,
        };

        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(RETENTION_SECS);

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Check manifest timestamp
            let manifest_path = path.join(MANIFEST_FILE);
            let should_remove = if manifest_path.exists() {
                manifest_path
                    .metadata()
                    .and_then(|m| m.modified())
                    .map(|mtime| mtime < cutoff)
                    .unwrap_or(false)
            } else {
                // No manifest — check directory mtime
                path.metadata()
                    .and_then(|m| m.modified())
                    .map(|mtime| mtime < cutoff)
                    .unwrap_or(false)
            };

            if should_remove {
                info!("Removing old checkpoint session: {}", path.display());
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    /// Enforce the maximum snapshot cap by pruning oldest turns.
    fn enforce_snapshot_cap(&mut self) {
        let total_snapshots: usize = self.turns.iter().map(|t| t.files.len()).sum();
        if total_snapshots <= MAX_SNAPSHOTS {
            return;
        }

        // Remove oldest turns until under the cap
        while !self.turns.is_empty() {
            let count: usize = self.turns.iter().map(|t| t.files.len()).sum();
            if count <= MAX_SNAPSHOTS {
                break;
            }

            let removed = self.turns.remove(0);
            // Clean up backup files for the removed turn
            for snapshot in &removed.files {
                if let Some(ref name) = snapshot.backup_file {
                    let _ = std::fs::remove_file(self.base_dir.join(name));
                }
            }
            debug!("Pruned turn {} to enforce snapshot cap", removed.turn_id);
        }
    }

    /// Load manifest from disk if it exists.
    fn load_manifest(&mut self) {
        let manifest_path = self.base_dir.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return;
        }

        let data = match std::fs::read_to_string(&manifest_path) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to read checkpoint manifest: {}", e);
                return;
            }
        };

        let manifest: Manifest = match serde_json::from_str(&data) {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to parse checkpoint manifest: {}", e);
                return;
            }
        };

        self.turns = manifest.turns;
        self.next_turn_id = manifest.next_turn_id;
        self.next_version = manifest.next_version;
        debug!("Loaded checkpoint manifest: {} turns", self.turns.len());
    }

    /// Persist manifest to disk.
    fn save_manifest(&self) {
        if let Err(e) = std::fs::create_dir_all(&self.base_dir) {
            warn!("Failed to create checkpoint dir: {}", e);
            return;
        }

        let manifest = Manifest {
            session_id: self.session_id.clone(),
            turns: self.turns.clone(),
            next_turn_id: self.next_turn_id,
            next_version: self.next_version.clone(),
        };

        let data = match serde_json::to_string_pretty(&manifest) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to serialize checkpoint manifest: {}", e);
                return;
            }
        };

        // Atomic write
        let tmp_path = self
            .base_dir
            .join(format!(".manifest.{}.tmp", uuid::Uuid::new_v4()));
        if let Err(e) = std::fs::write(&tmp_path, &data) {
            warn!("Failed to write checkpoint manifest: {}", e);
            return;
        }
        if let Err(e) = std::fs::rename(&tmp_path, self.base_dir.join(MANIFEST_FILE)) {
            warn!("Failed to finalize checkpoint manifest: {}", e);
            let _ = std::fs::remove_file(&tmp_path);
        }
    }
}

impl std::fmt::Debug for FileCheckpointManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileCheckpointManager")
            .field("session_id", &self.session_id)
            .field("turns", &self.turns.len())
            .field("base_dir", &self.base_dir)
            .finish()
    }
}

#[cfg(test)]
#[path = "file_checkpoint_tests.rs"]
mod tests;
