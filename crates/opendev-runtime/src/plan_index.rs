//! Plan index manager for tracking plan-session-project associations.
//!
//! Stores a lightweight JSON index at `~/.opendev/plans/plans-index.json`
//! following atomic-write patterns (tempfile + rename).
//!
//! Ported from `opendev/core/runtime/plan_index.py`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use tracing::warn;

const INDEX_FILE: &str = "plans-index.json";
const VERSION: u32 = 1;

/// A single plan entry in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub name: String,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "projectPath", skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub created: String,
}

/// On-disk format for plans-index.json.
#[derive(Debug, Serialize, Deserialize)]
struct IndexData {
    version: u32,
    entries: Vec<PlanEntry>,
}

impl Default for IndexData {
    fn default() -> Self {
        Self {
            version: VERSION,
            entries: Vec::new(),
        }
    }
}

/// Manage the plans-index.json file for plan-session-project tracking.
pub struct PlanIndex {
    plans_dir: PathBuf,
    index_path: PathBuf,
}

impl PlanIndex {
    /// Create a new plan index manager.
    ///
    /// # Arguments
    /// * `plans_dir` - Directory containing plan files (e.g. `~/.opendev/plans/`).
    pub fn new(plans_dir: impl Into<PathBuf>) -> Self {
        let dir = plans_dir.into();
        let index_path = dir.join(INDEX_FILE);
        Self {
            plans_dir: dir,
            index_path,
        }
    }

    /// Read the index file, returning default structure if missing or invalid.
    fn read_index(&self) -> IndexData {
        if !self.index_path.exists() {
            return IndexData::default();
        }
        match std::fs::read_to_string(&self.index_path) {
            Ok(content) => serde_json::from_str::<IndexData>(&content).unwrap_or_default(),
            Err(_) => IndexData::default(),
        }
    }

    /// Atomically write the index file (tempfile + rename).
    fn write_index(&self, data: &IndexData) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.plans_dir)?;

        let tmp_path = self.plans_dir.join(".plans-idx-tmp");
        {
            let mut f = std::fs::File::create(&tmp_path)?;
            let json = serde_json::to_string_pretty(data).map_err(std::io::Error::other)?;
            f.write_all(json.as_bytes())?;
            f.write_all(b"\n")?;
            f.sync_all()?;
        }

        std::fs::rename(&tmp_path, &self.index_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })
    }

    /// Add or update an entry in the plan index.
    ///
    /// If an entry with the same name already exists, it is replaced (upsert).
    pub fn add_entry(&self, name: &str, session_id: Option<&str>, project_path: Option<&str>) {
        let mut data = self.read_index();

        // Upsert: remove existing entry with same name
        data.entries.retain(|e| e.name != name);

        data.entries.push(PlanEntry {
            name: name.to_string(),
            session_id: session_id.map(|s| s.to_string()),
            project_path: project_path.map(|s| s.to_string()),
            created: Utc::now().to_rfc3339(),
        });

        if let Err(e) = self.write_index(&data) {
            warn!("Failed to write plan index: {}", e);
        }
    }

    /// Look up a plan entry by session ID.
    pub fn get_by_session(&self, session_id: &str) -> Option<PlanEntry> {
        self.read_index()
            .entries
            .into_iter()
            .find(|e| e.session_id.as_deref() == Some(session_id))
    }

    /// List all plan entries for a project.
    pub fn get_by_project(&self, project_path: &str) -> Vec<PlanEntry> {
        self.read_index()
            .entries
            .into_iter()
            .filter(|e| e.project_path.as_deref() == Some(project_path))
            .collect()
    }

    /// Remove an entry by plan name.
    pub fn remove_entry(&self, name: &str) {
        let mut data = self.read_index();
        data.entries.retain(|e| e.name != name);
        if let Err(e) = self.write_index(&data) {
            warn!("Failed to write plan index: {}", e);
        }
    }

    /// List all entries in the index.
    pub fn list_all(&self) -> Vec<PlanEntry> {
        self.read_index().entries
    }
}

#[cfg(test)]
#[path = "plan_index_tests.rs"]
mod tests;
