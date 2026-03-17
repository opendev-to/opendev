//! File I/O for persistent approval rules.

use super::types::{ApprovalRule, RuleScope};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// On-disk format for permissions.json.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PermissionsFile {
    pub version: u32,
    pub rules: Vec<ApprovalRule>,
}

/// User-global permissions path: `~/.opendev/permissions.json`.
pub(crate) fn user_permissions_path() -> Option<PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".opendev").join("permissions.json"))
}

/// Load persistent rules from both user-global and project-scoped files.
pub(crate) fn load_persistent_rules(rules: &mut Vec<ApprovalRule>, project_dir: Option<&Path>) {
    // User-global rules
    if let Some(path) = user_permissions_path() {
        load_rules_from_file(rules, &path);
    }
    // Project-scoped rules (higher priority, loaded second)
    if let Some(dir) = project_dir {
        load_rules_from_file(rules, &dir.join(".opendev").join("permissions.json"));
    }
}

fn load_rules_from_file(rules: &mut Vec<ApprovalRule>, path: &Path) {
    if !path.exists() {
        return;
    }
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<PermissionsFile>(&content) {
            Ok(data) => {
                let count = data.rules.len();
                for rule in data.rules {
                    // Skip duplicates
                    if rules.iter().any(|r| r.id == rule.id) {
                        continue;
                    }
                    rules.push(rule);
                }
                debug!("Loaded {} rules from {}", count, path.display());
            }
            Err(e) => {
                warn!(
                    "Failed to parse persistent rules from {}: {}",
                    path.display(),
                    e
                );
            }
        },
        Err(e) => {
            warn!(
                "Failed to read persistent rules from {}: {}",
                path.display(),
                e
            );
        }
    }
}

/// Save persistent (non-default) rules to disk for the given scope.
pub(crate) fn save_persistent_rules(
    rules: &[ApprovalRule],
    project_dir: Option<&Path>,
    scope: RuleScope,
) {
    let persistent: Vec<&ApprovalRule> = rules
        .iter()
        .filter(|r| !r.id.starts_with("default_"))
        .collect();
    let data = PermissionsFile {
        version: 1,
        rules: persistent.into_iter().cloned().collect(),
    };

    let path = match scope {
        RuleScope::User => user_permissions_path(),
        RuleScope::Project => project_dir.map(|d| d.join(".opendev").join("permissions.json")),
        RuleScope::All => {
            // Save to both
            save_persistent_rules(rules, project_dir, RuleScope::User);
            if project_dir.is_some() {
                save_persistent_rules(rules, project_dir, RuleScope::Project);
            }
            return;
        }
    };

    let Some(path) = path else { return };

    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        warn!("Failed to create directory {}: {}", parent.display(), e);
        return;
    }

    match serde_json::to_string_pretty(&data) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                warn!(
                    "Failed to save persistent rules to {}: {}",
                    path.display(),
                    e
                );
            } else {
                debug!("Saved {} rules to {}", data.rules.len(), path.display());
            }
        }
        Err(e) => {
            warn!("Failed to serialize rules: {}", e);
        }
    }
}

/// Delete a permissions file if it exists.
pub(crate) fn delete_permissions_file(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}
