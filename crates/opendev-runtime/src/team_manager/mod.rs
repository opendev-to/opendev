//! Agent team lifecycle manager.
//!
//! Creates named teams of agents that communicate via the mailbox system.
//! Teams have a leader (the main agent) and member agents that run as
//! background tasks.
//!
//! Storage: `~/.opendev/teams/{name}/team.json`

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::now_ms;

/// Persisted team configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub name: String,
    pub leader: String,
    pub leader_session_id: String,
    pub members: Vec<TeamMember>,
    pub created_at_ms: u64,
}

/// A single team member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    pub agent_type: String,
    pub task_id: String,
    pub task: String,
    pub status: TeamMemberStatus,
    pub joined_at_ms: u64,
}

/// Status of a team member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamMemberStatus {
    Idle,
    Busy,
    Waiting,
    Done,
    Failed,
}

impl std::fmt::Display for TeamMemberStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Busy => write!(f, "busy"),
            Self::Waiting => write!(f, "waiting"),
            Self::Done => write!(f, "done"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Manages agent teams.
pub struct TeamManager {
    teams_dir: PathBuf,
    active_teams: RwLock<HashMap<String, TeamConfig>>,
}

impl TeamManager {
    /// Create a new team manager.
    ///
    /// `teams_dir` is typically `~/.opendev/teams/`.
    pub fn new(teams_dir: PathBuf) -> Self {
        Self {
            teams_dir,
            active_teams: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new team.
    pub fn create_team(
        &self,
        name: &str,
        leader: &str,
        leader_session_id: &str,
    ) -> std::io::Result<TeamConfig> {
        let team_dir = self.teams_dir.join(name);
        fs::create_dir_all(&team_dir)?;
        fs::create_dir_all(team_dir.join("inboxes"))?;

        let config = TeamConfig {
            name: name.to_string(),
            leader: leader.to_string(),
            leader_session_id: leader_session_id.to_string(),
            members: Vec::new(),
            created_at_ms: now_ms(),
        };

        // Persist to disk
        let config_path = team_dir.join("team.json");
        let json = serde_json::to_string_pretty(&config).map_err(std::io::Error::other)?;

        // Write to temp file then rename (atomic)
        let tmp_path = config_path.with_extension(format!("tmp.{}", crate::now_ms()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);
            std::io::Write::write_all(&mut opts.open(&tmp_path)?, json.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            fs::write(&tmp_path, json)?;
        }

        fs::rename(&tmp_path, &config_path)?;

        let mut teams = self
            .active_teams
            .write()
            .expect("TeamManager lock poisoned");
        teams.insert(name.to_string(), config.clone());

        Ok(config)
    }

    /// Add a member to an existing team.
    pub fn add_member(&self, team_name: &str, member: TeamMember) -> std::io::Result<()> {
        let mut teams = self
            .active_teams
            .write()
            .expect("TeamManager lock poisoned");
        if let Some(config) = teams.get_mut(team_name) {
            config.members.push(member);
            self.persist_config(config)?;
        }
        Ok(())
    }

    /// Update a member's status.
    pub fn update_member_status(
        &self,
        team_name: &str,
        member_name: &str,
        status: TeamMemberStatus,
    ) {
        let mut teams = self
            .active_teams
            .write()
            .expect("TeamManager lock poisoned");
        if let Some(config) = teams.get_mut(team_name) {
            if let Some(member) = config.members.iter_mut().find(|m| m.name == member_name) {
                member.status = status;
            }
            let _ = self.persist_config(config);
        }
    }

    /// Delete a team and clean up its files.
    pub fn delete_team(&self, name: &str) -> std::io::Result<()> {
        let team_dir = self.teams_dir.join(name);
        if team_dir.exists() {
            fs::remove_dir_all(&team_dir)?;
        }
        let mut teams = self
            .active_teams
            .write()
            .expect("TeamManager lock poisoned");
        teams.remove(name);
        Ok(())
    }

    /// Get a team config by name.
    pub fn get_team(&self, name: &str) -> Option<TeamConfig> {
        let teams = self.active_teams.read().expect("TeamManager lock poisoned");
        teams.get(name).cloned()
    }

    /// List all active teams.
    pub fn list_teams(&self) -> Vec<TeamConfig> {
        let teams = self.active_teams.read().expect("TeamManager lock poisoned");
        teams.values().cloned().collect()
    }

    /// Get the directory for a team (for mailbox paths).
    pub fn team_dir(&self, name: &str) -> PathBuf {
        self.teams_dir.join(name)
    }

    /// Persist a team config to disk.
    fn persist_config(&self, config: &TeamConfig) -> std::io::Result<()> {
        let config_path = self.teams_dir.join(&config.name).join("team.json");
        let json = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;

        let tmp_path = config_path.with_extension(format!("tmp.{}", crate::now_ms()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);
            std::io::Write::write_all(&mut opts.open(&tmp_path)?, json.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            fs::write(&tmp_path, json)?;
        }

        fs::rename(&tmp_path, &config_path)?;
        Ok(())
    }

    /// Clean up orphaned teams (teams whose leader session no longer exists).
    pub fn cleanup_orphans(&self) {
        if let Ok(entries) = fs::read_dir(&self.teams_dir) {
            for entry in entries.flatten() {
                let config_path = entry.path().join("team.json");
                if config_path.exists()
                    && let Ok(content) = fs::read_to_string(&config_path)
                    && serde_json::from_str::<TeamConfig>(&content).is_err()
                {
                    warn!(
                        path = %config_path.display(),
                        "Removing corrupt team config"
                    );
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
    }
}

impl std::fmt::Debug for TeamManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.active_teams.read().map(|t| t.len()).unwrap_or(0);
        f.debug_struct("TeamManager")
            .field("teams_dir", &self.teams_dir)
            .field("team_count", &count)
            .finish()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
