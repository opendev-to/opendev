//! Permission-related configuration models.

use regex::Regex;
use serde::{Deserialize, Serialize};

pub(super) fn default_true() -> bool {
    true
}

/// Permission settings for a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermission {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub always_allow: bool,
    #[serde(default)]
    pub deny_patterns: Vec<String>,
}

impl Default for ToolPermission {
    fn default() -> Self {
        Self {
            enabled: true,
            always_allow: false,
            deny_patterns: Vec::new(),
        }
    }
}

impl ToolPermission {
    /// Check if a target (file path, command, etc.) is allowed.
    pub fn is_allowed(&self, target: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if self.always_allow {
            return true;
        }
        !self.deny_patterns.iter().any(|pattern| {
            Regex::new(pattern)
                .map(|re| re.is_match(target))
                .unwrap_or(false)
        })
    }
}

/// Global permission configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    #[serde(default)]
    pub file_write: ToolPermission,
    #[serde(default)]
    pub file_read: ToolPermission,
    #[serde(default = "default_bash_permission")]
    pub bash: ToolPermission,
    #[serde(default)]
    pub git: ToolPermission,
    #[serde(default)]
    pub web_fetch: ToolPermission,
}

fn default_bash_permission() -> ToolPermission {
    ToolPermission {
        enabled: true,
        always_allow: false,
        deny_patterns: vec![
            "rm -rf /".to_string(),
            "sudo rm -rf /*".to_string(),
            "chmod -R 777 /*".to_string(),
        ],
    }
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            file_write: ToolPermission::default(),
            file_read: ToolPermission::default(),
            bash: default_bash_permission(),
            git: ToolPermission::default(),
            web_fetch: ToolPermission::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_permission_is_allowed() {
        let perm = ToolPermission {
            enabled: true,
            always_allow: false,
            deny_patterns: vec!["rm -rf /".to_string()],
        };
        assert!(perm.is_allowed("ls -la"));
        assert!(!perm.is_allowed("rm -rf /"));

        let disabled = ToolPermission {
            enabled: false,
            ..Default::default()
        };
        assert!(!disabled.is_allowed("anything"));

        let allow_all = ToolPermission {
            enabled: true,
            always_allow: true,
            deny_patterns: vec![".*".to_string()],
        };
        assert!(allow_all.is_allowed("anything"));
    }
}
