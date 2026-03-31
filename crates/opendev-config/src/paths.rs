//! Centralized path management for OpenDev.

use std::path::{Path, PathBuf};

// Directory and file names
pub const APP_DIR_NAME: &str = ".opendev";
pub const MCP_CONFIG_NAME: &str = "mcp.json";
pub const MCP_PROJECT_CONFIG_NAME: &str = ".mcp.json";
pub const SESSIONS_DIR_NAME: &str = "sessions";
pub const PROJECTS_DIR_NAME: &str = "projects";
pub const PLANS_DIR_NAME: &str = "plans";
pub const LOGS_DIR_NAME: &str = "logs";
pub const CACHE_DIR_NAME: &str = "cache";
pub const SKILLS_DIR_NAME: &str = "skills";
pub const AGENTS_DIR_NAME: &str = "agents";
pub const COMMANDS_DIR_NAME: &str = "commands";
pub const REPOS_DIR_NAME: &str = "repos";
pub const PLUGINS_DIR_NAME: &str = "plugins";
pub const MARKETPLACES_DIR_NAME: &str = "marketplaces";
pub const BUNDLES_DIR_NAME: &str = "bundles";
pub const PLUGIN_CACHE_DIR_NAME: &str = "cache";
pub const SETTINGS_FILE_NAME: &str = "settings.json";
pub const SESSIONS_INDEX_FILE_NAME: &str = "sessions-index.json";
pub const AGENTS_FILE_NAME: &str = "agents.json";
pub const CONTEXT_FILE_NAME: &str = "AGENTS.md";
pub const HISTORY_FILE_NAME: &str = "history.txt";
pub const KNOWN_MARKETPLACES_FILE_NAME: &str = "known_marketplaces.json";
pub const INSTALLED_PLUGINS_FILE_NAME: &str = "installed_plugins.json";
pub const BUNDLES_FILE_NAME: &str = "bundles.json";
pub const MEMORY_DIR_NAME: &str = "memory";

// Environment variable names for overrides
pub const ENV_OPENDEV_DIR: &str = "OPENDEV_DIR";
pub const ENV_OPENDEV_SESSION_DIR: &str = "OPENDEV_SESSION_DIR";
pub const ENV_OPENDEV_LOG_DIR: &str = "OPENDEV_LOG_DIR";
pub const ENV_OPENDEV_CACHE_DIR: &str = "OPENDEV_CACHE_DIR";

/// Encode an absolute path into a directory-safe string.
///
/// Replaces `/` with `-` so the result can be used as a single directory name.
/// E.g., `/Users/foo/bar` becomes `-Users-foo-bar`.
pub fn encode_project_path(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    resolved.to_string_lossy().replace('/', "-")
}

/// Centralized path management.
///
/// Provides access to all application paths with support for:
/// - Global paths (~/.opendev/...)
/// - Project paths (<working_dir>/.opendev/...)
/// - Environment variable overrides
#[derive(Debug, Clone)]
pub struct Paths {
    working_dir: PathBuf,
}

impl Paths {
    /// Create a new Paths instance.
    pub fn new(working_dir: Option<PathBuf>) -> Self {
        Self {
            working_dir: working_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
        }
    }

    /// Get the working directory.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    // ========================================================================
    // Global Paths (User-level, in ~/.opendev/)
    // ========================================================================

    /// Get the global opendev directory. Can be overridden with OPENDEV_DIR.
    pub fn global_dir(&self) -> PathBuf {
        if let Ok(override_dir) = std::env::var(ENV_OPENDEV_DIR) {
            return PathBuf::from(override_dir);
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(APP_DIR_NAME)
    }

    /// Get global settings file path.
    pub fn global_settings(&self) -> PathBuf {
        self.global_dir().join(SETTINGS_FILE_NAME)
    }

    /// Get global sessions directory.
    pub fn global_sessions_dir(&self) -> PathBuf {
        if let Ok(override_dir) = std::env::var(ENV_OPENDEV_SESSION_DIR) {
            return PathBuf::from(override_dir);
        }
        self.global_dir().join(SESSIONS_DIR_NAME)
    }

    /// Get global projects directory for project-scoped sessions.
    pub fn global_projects_dir(&self) -> PathBuf {
        self.global_dir().join(PROJECTS_DIR_NAME)
    }

    /// Get the project-scoped sessions directory for a given working directory.
    pub fn project_sessions_dir(&self, working_dir: &Path) -> PathBuf {
        let encoded = encode_project_path(working_dir);
        self.global_projects_dir().join(encoded)
    }

    /// Get global logs directory.
    pub fn global_logs_dir(&self) -> PathBuf {
        if let Ok(override_dir) = std::env::var(ENV_OPENDEV_LOG_DIR) {
            return PathBuf::from(override_dir);
        }
        self.global_dir().join(LOGS_DIR_NAME)
    }

    /// Get global cache directory.
    pub fn global_cache_dir(&self) -> PathBuf {
        if let Ok(override_dir) = std::env::var(ENV_OPENDEV_CACHE_DIR) {
            return PathBuf::from(override_dir);
        }
        self.global_dir().join(CACHE_DIR_NAME)
    }

    /// Get provider cache directory (`~/.opendev/cache/providers/`).
    ///
    /// Stores cached model/provider JSON from models.dev API with 24h TTL.
    pub fn providers_cache_dir(&self) -> PathBuf {
        self.global_cache_dir().join("providers")
    }

    /// Get global skills directory.
    pub fn global_skills_dir(&self) -> PathBuf {
        self.global_dir().join(SKILLS_DIR_NAME)
    }

    /// Get global agents directory.
    pub fn global_agents_dir(&self) -> PathBuf {
        self.global_dir().join(AGENTS_DIR_NAME)
    }

    /// Get global agents.json file path.
    pub fn global_agents_file(&self) -> PathBuf {
        self.global_dir().join(AGENTS_FILE_NAME)
    }

    /// Get global context file (AGENTS.md) path.
    pub fn global_context_file(&self) -> PathBuf {
        self.global_dir().join(CONTEXT_FILE_NAME)
    }

    /// Get global MCP configuration file path.
    pub fn global_mcp_config(&self) -> PathBuf {
        self.global_dir().join(MCP_CONFIG_NAME)
    }

    /// Get global plans directory.
    pub fn global_plans_dir(&self) -> PathBuf {
        self.global_dir().join(PLANS_DIR_NAME)
    }

    /// Get global repos directory.
    pub fn global_repos_dir(&self) -> PathBuf {
        self.global_dir().join(REPOS_DIR_NAME)
    }

    /// Get global command history file path.
    pub fn global_history_file(&self) -> PathBuf {
        self.global_dir().join(HISTORY_FILE_NAME)
    }

    // ========================================================================
    // Memory Paths
    // ========================================================================

    /// Get global memory directory (`~/.opendev/memory/`).
    pub fn global_memory_dir(&self) -> PathBuf {
        self.global_dir().join(MEMORY_DIR_NAME)
    }

    /// Get the project-scoped memory directory (`~/.opendev/projects/{encoded}/memory/`).
    pub fn project_memory_dir(&self) -> PathBuf {
        let encoded = encode_project_path(&self.working_dir);
        self.global_projects_dir()
            .join(encoded)
            .join(MEMORY_DIR_NAME)
    }

    /// Get the project MEMORY.md index file path.
    pub fn project_memory_index(&self) -> PathBuf {
        self.project_memory_dir().join("MEMORY.md")
    }

    /// Get the project-scoped file history path.
    pub fn project_file_history(&self) -> PathBuf {
        let encoded = encode_project_path(&self.working_dir);
        self.global_projects_dir()
            .join(encoded)
            .join("file-history.json")
    }

    // ========================================================================
    // Plugin Paths
    // ========================================================================

    /// Get global plugins directory.
    pub fn global_plugins_dir(&self) -> PathBuf {
        self.global_dir().join(PLUGINS_DIR_NAME)
    }

    /// Get global marketplaces directory.
    pub fn global_marketplaces_dir(&self) -> PathBuf {
        self.global_plugins_dir().join(MARKETPLACES_DIR_NAME)
    }

    /// Get global plugin cache directory.
    pub fn global_plugin_cache_dir(&self) -> PathBuf {
        self.global_plugins_dir().join(PLUGIN_CACHE_DIR_NAME)
    }

    /// Get known marketplaces registry file.
    pub fn known_marketplaces_file(&self) -> PathBuf {
        self.global_plugins_dir().join(KNOWN_MARKETPLACES_FILE_NAME)
    }

    /// Get global installed plugins registry file.
    pub fn global_installed_plugins_file(&self) -> PathBuf {
        self.global_plugins_dir().join(INSTALLED_PLUGINS_FILE_NAME)
    }

    /// Get global bundles directory.
    pub fn global_bundles_dir(&self) -> PathBuf {
        self.global_plugins_dir().join(BUNDLES_DIR_NAME)
    }

    /// Get global bundles registry file.
    pub fn global_bundles_file(&self) -> PathBuf {
        self.global_plugins_dir().join(BUNDLES_FILE_NAME)
    }

    // ========================================================================
    // Project Paths
    // ========================================================================

    /// Get project-level opendev directory.
    pub fn project_dir(&self) -> PathBuf {
        self.working_dir.join(APP_DIR_NAME)
    }

    /// Get project settings file path.
    pub fn project_settings(&self) -> PathBuf {
        self.project_dir().join(SETTINGS_FILE_NAME)
    }

    /// Get project skills directory.
    pub fn project_skills_dir(&self) -> PathBuf {
        self.project_dir().join(SKILLS_DIR_NAME)
    }

    /// Get project agents directory.
    pub fn project_agents_dir(&self) -> PathBuf {
        self.project_dir().join(AGENTS_DIR_NAME)
    }

    /// Get project agents.json file path.
    pub fn project_agents_file(&self) -> PathBuf {
        self.project_dir().join(AGENTS_FILE_NAME)
    }

    /// Get project commands directory.
    pub fn project_commands_dir(&self) -> PathBuf {
        self.project_dir().join(COMMANDS_DIR_NAME)
    }

    /// Get project context file (AGENTS.md) path at project root.
    pub fn project_context_file(&self) -> PathBuf {
        self.working_dir.join(CONTEXT_FILE_NAME)
    }

    /// Get project MCP configuration file path at project root.
    pub fn project_mcp_config(&self) -> PathBuf {
        self.working_dir.join(MCP_PROJECT_CONFIG_NAME)
    }

    /// Get project plugins directory.
    pub fn project_plugins_dir(&self) -> PathBuf {
        self.project_dir().join(PLUGINS_DIR_NAME)
    }

    /// Get path to a specific session file.
    pub fn session_file(&self, session_id: &str) -> PathBuf {
        self.global_sessions_dir()
            .join(format!("{session_id}.json"))
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Create all required global directories.
    pub fn ensure_global_dirs(&self) -> std::io::Result<()> {
        let dirs = [
            self.global_dir(),
            self.global_sessions_dir(),
            self.global_projects_dir(),
            self.global_logs_dir(),
            self.global_cache_dir(),
            self.providers_cache_dir(),
            self.global_plans_dir(),
            self.global_skills_dir(),
            self.global_agents_dir(),
            self.global_plugins_dir(),
            self.global_marketplaces_dir(),
            self.global_plugin_cache_dir(),
            self.global_bundles_dir(),
        ];
        for dir in &dirs {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Create project directories if in a git repository.
    pub fn ensure_project_dirs(&self) -> std::io::Result<()> {
        if self.working_dir.join(".git").exists() {
            std::fs::create_dir_all(self.project_commands_dir())?;
        }
        Ok(())
    }

    /// Get all skill directories in priority order.
    pub fn get_skill_dirs(&self) -> Vec<PathBuf> {
        let candidates = [self.project_skills_dir(), self.global_skills_dir()];
        candidates.into_iter().filter(|p| p.exists()).collect()
    }

    /// Get all agents directories in priority order.
    pub fn get_agents_dirs(&self) -> Vec<PathBuf> {
        let candidates = [self.project_agents_dir(), self.global_agents_dir()];
        candidates.into_iter().filter(|p| p.exists()).collect()
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
