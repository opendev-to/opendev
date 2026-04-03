//! Plugin manager: discovery, install/uninstall, enable/disable.

use crate::models::{
    InstalledPlugins, KnownMarketplaces, PluginConfig, PluginManifest, PluginMetadata, PluginScope,
    PluginSource, PluginStatus,
};
use chrono::Utc;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, info, warn};

/// Errors produced by the plugin manager.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin already installed: {0}")]
    AlreadyInstalled(String),
    #[error("Marketplace not found: {0}")]
    MarketplaceNotFound(String),
    #[error("Marketplace already exists: {0}")]
    MarketplaceAlreadyExists(String),
    #[error("Invalid plugin: {0}")]
    InvalidPlugin(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Git operation failed: {0}")]
    Git(String),
    #[error("Plugin manager error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PluginError>;

/// Paths used by the plugin manager.
#[derive(Debug, Clone)]
pub struct PluginPaths {
    /// Global plugins directory: ~/.opendev/plugins/
    pub global_plugins_dir: PathBuf,
    /// Project plugins directory: .opendev/plugins/
    pub project_plugins_dir: PathBuf,
    /// Global marketplaces directory: ~/.opendev/marketplaces/
    pub global_marketplaces_dir: PathBuf,
    /// Global plugin cache: ~/.opendev/plugins/cache/
    pub global_plugin_cache_dir: PathBuf,
    /// Known marketplaces registry file.
    pub known_marketplaces_file: PathBuf,
    /// Global installed plugins registry file.
    pub global_installed_plugins_file: PathBuf,
    /// Project installed plugins registry file.
    pub project_installed_plugins_file: PathBuf,
}

impl PluginPaths {
    /// Build plugin paths from an optional working directory.
    pub fn new(working_dir: Option<&Path>) -> Self {
        let paths = opendev_config::Paths::new(working_dir.map(PathBuf::from));
        let project_base = working_dir
            .map(|d| d.join(".opendev"))
            .unwrap_or_else(|| PathBuf::from(".opendev"));

        Self {
            global_plugins_dir: paths.global_plugins_dir(),
            project_plugins_dir: project_base.join("plugins"),
            global_marketplaces_dir: paths.global_marketplaces_dir(),
            global_plugin_cache_dir: paths.global_plugin_cache_dir(),
            known_marketplaces_file: paths.known_marketplaces_file(),
            global_installed_plugins_file: paths.global_installed_plugins_file(),
            project_installed_plugins_file: project_base.join("installed_plugins.json"),
        }
    }
}

/// Plugin manager: discovers, installs, and manages plugins.
pub struct PluginManager {
    /// Working directory for path resolution.
    pub working_dir: Option<PathBuf>,
    /// Resolved paths.
    pub paths: PluginPaths,
}

impl PluginManager {
    /// Create a new PluginManager.
    pub fn new(working_dir: Option<PathBuf>) -> Self {
        let paths = PluginPaths::new(working_dir.as_deref());
        Self { working_dir, paths }
    }

    // ── Discovery ──────────────────────────────────────────────

    /// Discover plugins from both project and global plugin directories.
    /// Scans each directory for subdirectories containing a `manifest.json`.
    pub fn discover_plugins(&self) -> Result<Vec<PluginManifest>> {
        let mut manifests = Vec::new();

        // Project plugins (higher priority)
        if self.paths.project_plugins_dir.exists() {
            debug!(
                path = %self.paths.project_plugins_dir.display(),
                "Scanning project plugins directory"
            );
            self.scan_directory(&self.paths.project_plugins_dir, &mut manifests)?;
        }

        // Global plugins
        if self.paths.global_plugins_dir.exists() {
            debug!(
                path = %self.paths.global_plugins_dir.display(),
                "Scanning global plugins directory"
            );
            self.scan_directory(&self.paths.global_plugins_dir, &mut manifests)?;
        }

        info!(count = manifests.len(), "Discovered plugins");
        Ok(manifests)
    }

    /// Scan a directory for plugin subdirectories containing `manifest.json`.
    fn scan_directory(&self, dir: &Path, manifests: &mut Vec<PluginManifest>) -> Result<()> {
        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                match self.load_manifest(&path) {
                    Ok(manifest) => {
                        debug!(name = %manifest.name, "Found plugin");
                        manifests.push(manifest);
                    }
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Skipping directory: failed to load manifest"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Load a plugin manifest from a directory.
    /// Checks multiple possible locations for `manifest.json`.
    pub fn load_manifest(&self, plugin_dir: &Path) -> Result<PluginManifest> {
        let possible_paths = [
            plugin_dir.join(".opendev").join("manifest.json"),
            plugin_dir.join("manifest.json"),
            plugin_dir.join("plugin.json"),
        ];

        for path in &possible_paths {
            if path.exists() {
                let content = std::fs::read_to_string(path)?;
                let manifest: PluginManifest = serde_json::from_str(&content)?;
                return Ok(manifest);
            }
        }

        Err(PluginError::InvalidPlugin(format!(
            "No manifest.json found in {}",
            plugin_dir.display()
        )))
    }

    // ── Install / Uninstall ────────────────────────────────────

    /// Install a plugin from a marketplace into the plugins directory.
    pub fn install_plugin(
        &self,
        plugin_name: &str,
        marketplace_name: &str,
        scope: PluginScope,
    ) -> Result<PluginConfig> {
        // Check marketplace exists
        let marketplaces = self.load_known_marketplaces()?;
        if !marketplaces.marketplaces.contains_key(marketplace_name) {
            return Err(PluginError::MarketplaceNotFound(
                marketplace_name.to_string(),
            ));
        }

        // Check not already installed
        let installed = self.load_installed_plugins(scope)?;
        if installed.get(marketplace_name, plugin_name).is_some() {
            return Err(PluginError::AlreadyInstalled(format!(
                "{}:{}",
                marketplace_name, plugin_name
            )));
        }

        // Locate plugin in marketplace directory
        let marketplace_dir = self.paths.global_marketplaces_dir.join(marketplace_name);
        let source_dir = marketplace_dir.join("plugins").join(plugin_name);
        if !source_dir.exists() {
            return Err(PluginError::NotFound(format!(
                "Plugin '{}' not found in marketplace '{}'",
                plugin_name, marketplace_name
            )));
        }

        // Load manifest to get version
        let manifest = self.load_manifest(&source_dir)?;

        // Determine target directory
        let cache_dir = match scope {
            PluginScope::Project => self.paths.project_plugins_dir.join("cache"),
            PluginScope::User => self.paths.global_plugin_cache_dir.clone(),
        };
        let target_dir = cache_dir
            .join(marketplace_name)
            .join(plugin_name)
            .join(&manifest.version);

        // Copy plugin to cache
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)?;
        }
        copy_dir_recursive(&source_dir, &target_dir)?;

        // Register installation
        let config = PluginConfig {
            name: plugin_name.to_string(),
            version: manifest.version.clone(),
            source: PluginSource::Marketplace {
                marketplace: marketplace_name.to_string(),
            },
            status: PluginStatus::Installed,
            scope,
            enabled: true,
            path: target_dir,
            installed_at: Utc::now(),
            marketplace: Some(marketplace_name.to_string()),
        };

        let mut installed = self.load_installed_plugins(scope)?;
        installed.add(config.clone());
        self.save_installed_plugins(&installed, scope)?;

        info!(
            plugin = plugin_name,
            marketplace = marketplace_name,
            "Plugin installed"
        );
        Ok(config)
    }

    /// Uninstall a plugin.
    pub fn uninstall_plugin(
        &self,
        plugin_name: &str,
        marketplace_name: &str,
        scope: PluginScope,
    ) -> Result<()> {
        let mut installed = self.load_installed_plugins(scope)?;
        let plugin = installed
            .remove(marketplace_name, plugin_name)
            .ok_or_else(|| {
                PluginError::NotFound(format!(
                    "Plugin '{}:{}' not installed in {:?} scope",
                    marketplace_name, plugin_name, scope
                ))
            })?;

        // Remove from filesystem
        if plugin.path.exists() {
            std::fs::remove_dir_all(&plugin.path)?;
        }

        self.save_installed_plugins(&installed, scope)?;
        info!(plugin = plugin_name, "Plugin uninstalled");
        Ok(())
    }

    // ── Enable / Disable ───────────────────────────────────────

    /// Enable a disabled plugin.
    pub fn enable_plugin(
        &self,
        plugin_name: &str,
        marketplace_name: &str,
        scope: PluginScope,
    ) -> Result<()> {
        let mut installed = self.load_installed_plugins(scope)?;
        let plugin = installed
            .get_mut(marketplace_name, plugin_name)
            .ok_or_else(|| {
                PluginError::NotFound(format!(
                    "Plugin '{}:{}' not installed in {:?} scope",
                    marketplace_name, plugin_name, scope
                ))
            })?;

        plugin.enabled = true;
        plugin.status = PluginStatus::Installed;
        self.save_installed_plugins(&installed, scope)?;
        info!(plugin = plugin_name, "Plugin enabled");
        Ok(())
    }

    /// Disable a plugin.
    pub fn disable_plugin(
        &self,
        plugin_name: &str,
        marketplace_name: &str,
        scope: PluginScope,
    ) -> Result<()> {
        let mut installed = self.load_installed_plugins(scope)?;
        let plugin = installed
            .get_mut(marketplace_name, plugin_name)
            .ok_or_else(|| {
                PluginError::NotFound(format!(
                    "Plugin '{}:{}' not installed in {:?} scope",
                    marketplace_name, plugin_name, scope
                ))
            })?;

        plugin.enabled = false;
        plugin.status = PluginStatus::Disabled;
        self.save_installed_plugins(&installed, scope)?;
        info!(plugin = plugin_name, "Plugin disabled");
        Ok(())
    }

    // ── List ───────────────────────────────────────────────────

    /// List all installed plugins, optionally filtering by scope.
    pub fn list_installed(&self, scope: Option<PluginScope>) -> Result<Vec<PluginConfig>> {
        match scope {
            Some(s) => {
                let installed = self.load_installed_plugins(s)?;
                Ok(installed.plugins.into_values().collect())
            }
            None => {
                // Merge project + user, project takes priority
                let project = self.load_installed_plugins(PluginScope::Project)?;
                let user = self.load_installed_plugins(PluginScope::User)?;

                let mut all: Vec<PluginConfig> = project.plugins.values().cloned().collect();

                let project_keys: std::collections::HashSet<_> =
                    project.plugins.keys().cloned().collect();
                for (key, plugin) in &user.plugins {
                    if !project_keys.contains(key) {
                        all.push(plugin.clone());
                    }
                }
                Ok(all)
            }
        }
    }

    // ── Registry persistence ───────────────────────────────────

    /// Load the known marketplaces registry from disk.
    pub fn load_known_marketplaces(&self) -> Result<KnownMarketplaces> {
        let path = &self.paths.known_marketplaces_file;
        if !path.exists() {
            return Ok(KnownMarketplaces::default());
        }
        let content = std::fs::read_to_string(path)?;
        let marketplaces: KnownMarketplaces = serde_json::from_str(&content)?;
        Ok(marketplaces)
    }

    /// Save the known marketplaces registry to disk.
    pub fn save_known_marketplaces(&self, marketplaces: &KnownMarketplaces) -> Result<()> {
        let path = &self.paths.known_marketplaces_file;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(marketplaces)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load the installed plugins registry from disk.
    pub fn load_installed_plugins(&self, scope: PluginScope) -> Result<InstalledPlugins> {
        let path = match scope {
            PluginScope::User => &self.paths.global_installed_plugins_file,
            PluginScope::Project => &self.paths.project_installed_plugins_file,
        };
        if !path.exists() {
            return Ok(InstalledPlugins::default());
        }
        let content = std::fs::read_to_string(path)?;
        let plugins: InstalledPlugins = serde_json::from_str(&content)?;
        Ok(plugins)
    }

    /// Save the installed plugins registry to disk.
    pub fn save_installed_plugins(
        &self,
        plugins: &InstalledPlugins,
        scope: PluginScope,
    ) -> Result<()> {
        let path = match scope {
            PluginScope::User => &self.paths.global_installed_plugins_file,
            PluginScope::Project => &self.paths.project_installed_plugins_file,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(plugins)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    // ── Plugin metadata loading ────────────────────────────────

    /// Load plugin metadata from a plugin.json file in the given directory.
    pub fn load_plugin_metadata(&self, plugin_dir: &Path) -> Result<PluginMetadata> {
        let possible_paths = [
            plugin_dir.join(".opendev").join("plugin.json"),
            plugin_dir.join("plugin.json"),
        ];

        for path in &possible_paths {
            if path.exists() {
                let content = std::fs::read_to_string(path)?;
                let metadata: PluginMetadata = serde_json::from_str(&content)?;
                return Ok(metadata);
            }
        }

        Err(PluginError::InvalidPlugin(format!(
            "No plugin.json found in {}",
            plugin_dir.display()
        )))
    }

    /// Parse SKILL.md frontmatter for name and description.
    pub fn parse_skill_metadata(skill_file: &Path) -> (String, String) {
        let content = match std::fs::read_to_string(skill_file) {
            Ok(c) => c,
            Err(_) => return (String::new(), String::new()),
        };

        let mut name = String::new();
        let mut description = String::new();

        if content.starts_with("---") {
            let parts: Vec<&str> = content.splitn(3, "---").collect();
            if parts.len() >= 3 {
                for line in parts[1].trim().lines() {
                    let line = line.trim();
                    if let Some(val) = line.strip_prefix("name:") {
                        name = val
                            .trim()
                            .trim_matches(|c| c == '"' || c == '\'')
                            .to_string();
                    } else if let Some(val) = line.strip_prefix("description:") {
                        description = val
                            .trim()
                            .trim_matches(|c| c == '"' || c == '\'')
                            .to_string();
                    }
                }
            }
        }

        (name, description)
    }

    /// Discover skill names in a plugin directory.
    pub fn discover_skills_in_dir(plugin_dir: &Path) -> Vec<String> {
        let skills_dir = plugin_dir.join("skills");
        let mut skills = Vec::new();
        if skills_dir.exists()
            && skills_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&skills_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && path.join("SKILL.md").exists()
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                {
                    skills.push(name.to_string());
                }
            }
        }
        skills
    }

    /// Extract a name from a git URL.
    pub fn extract_name_from_url(url: &str) -> String {
        // Remove .git suffix
        let cleaned = regex::Regex::new(r"\.git$")
            .unwrap()
            .replace(url, "")
            .to_string();

        // Try to parse as URL
        if let Ok(parsed) = url::Url::parse(&cleaned) {
            let path = parsed.path().trim_matches('/');
            if let Some(last) = path.split('/').next_back() {
                let name = last.to_string();
                let name = regex::Regex::new(r"^swecli-")
                    .unwrap()
                    .replace(&name, "")
                    .to_string();
                let name = regex::Regex::new(r"-marketplace$")
                    .unwrap()
                    .replace(&name, "")
                    .to_string();
                if !name.is_empty() {
                    return name;
                }
            }
        }

        // Handle SSH-style URLs: git@github.com:user/repo
        if cleaned.contains('@')
            && cleaned.contains(':')
            && let Some(path_part) = cleaned.split(':').next_back()
            && let Some(last) = path_part.trim_matches('/').split('/').next_back()
        {
            let name = last.to_string();
            let name = regex::Regex::new(r"^swecli-")
                .unwrap()
                .replace(&name, "")
                .to_string();
            let name = regex::Regex::new(r"-marketplace$")
                .unwrap()
                .replace(&name, "")
                .to_string();
            if !name.is_empty() {
                return name;
            }
        }

        "default".to_string()
    }
}

/// Recursively copy a directory.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
