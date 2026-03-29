//! Skill loading from remote URLs and local cache.
//!
//! Skills are markdown files that define agent behaviors. This module adds
//! support for loading skills from HTTPS URLs with local caching in
//! `~/.opendev/skills/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::debug;

/// Errors that can occur when loading skills.
#[derive(Debug, Error)]
pub enum SkillError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("skill parse error: {0}")]
    ParseError(String),
    #[error("URL scheme must be https: {0}")]
    InsecureUrl(String),
}

/// A parsed skill definition loaded from a markdown file.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill name (derived from the first heading or filename).
    pub name: String,
    /// Raw markdown content.
    pub content: String,
    /// Source URL (if loaded from a remote URL).
    pub source_url: Option<String>,
    /// Local cache path.
    pub cache_path: Option<PathBuf>,
    /// Extracted sections from the markdown.
    pub sections: HashMap<String, String>,
}

/// Default cache directory for downloaded skills.
fn default_skills_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".opendev")
        .join("skills")
}

/// Generate a cache filename from a URL.
fn url_to_cache_filename(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let hash = hasher.finalize();
    let hex: String = hash.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{hex}.md")
}

/// Parse a markdown string into a `Skill`.
///
/// Extracts the skill name from the first `#` heading, and splits the
/// document into sections keyed by heading text.
pub fn parse_skill(content: &str, fallback_name: &str) -> Result<Skill, SkillError> {
    if content.trim().is_empty() {
        return Err(SkillError::ParseError("skill content is empty".to_string()));
    }

    let mut name = fallback_name.to_string();
    let mut sections: HashMap<String, String> = HashMap::new();
    let mut current_section = String::new();
    let mut current_body = String::new();

    for line in content.lines() {
        if let Some(heading) = line.strip_prefix("# ") {
            // First heading becomes the skill name
            if name == fallback_name && !heading.trim().is_empty() {
                name = heading.trim().to_string();
            }
            // Save previous section
            if !current_section.is_empty() {
                sections.insert(current_section.clone(), current_body.trim().to_string());
            }
            current_section = heading.trim().to_string();
            current_body.clear();
        } else if let Some(heading) = line.strip_prefix("## ") {
            if !current_section.is_empty() {
                sections.insert(current_section.clone(), current_body.trim().to_string());
            }
            current_section = heading.trim().to_string();
            current_body.clear();
        } else {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }

    // Save last section
    if !current_section.is_empty() {
        sections.insert(current_section, current_body.trim().to_string());
    }

    Ok(Skill {
        name,
        content: content.to_string(),
        source_url: None,
        cache_path: None,
        sections,
    })
}

/// Load a skill from an HTTPS URL.
///
/// The skill markdown is fetched, cached locally in `~/.opendev/skills/`,
/// and parsed into a [`Skill`]. Subsequent calls for the same URL will
/// return the cached version unless `force_refresh` is true.
pub fn load_skill_from_url(url: &str) -> Result<Skill, SkillError> {
    load_skill_from_url_with_options(url, None, false)
}

/// Load a skill from a URL with options for cache directory and force refresh.
pub fn load_skill_from_url_with_options(
    url: &str,
    cache_dir: Option<&Path>,
    force_refresh: bool,
) -> Result<Skill, SkillError> {
    // Validate URL
    if !url.starts_with("https://") {
        return Err(SkillError::InsecureUrl(url.to_string()));
    }

    if !url.contains('.') || url.len() < 12 {
        return Err(SkillError::InvalidUrl(url.to_string()));
    }

    let skills_dir = cache_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_skills_dir);
    let cache_filename = url_to_cache_filename(url);
    let cache_path = skills_dir.join(&cache_filename);

    // Try cache first (unless force refresh)
    if !force_refresh && cache_path.exists() {
        debug!("Loading cached skill from {:?}", cache_path);
        let content = std::fs::read_to_string(&cache_path)?;
        let fallback_name = extract_name_from_url(url);
        let mut skill = parse_skill(&content, &fallback_name)?;
        skill.source_url = Some(url.to_string());
        skill.cache_path = Some(cache_path);
        return Ok(skill);
    }

    // Fetch from URL
    debug!("Fetching skill from {}", url);
    let content = fetch_url_content(url)?;

    // Cache locally
    std::fs::create_dir_all(&skills_dir)?;
    std::fs::write(&cache_path, &content)?;
    debug!("Cached skill to {:?}", cache_path);

    let fallback_name = extract_name_from_url(url);
    let mut skill = parse_skill(&content, &fallback_name)?;
    skill.source_url = Some(url.to_string());
    skill.cache_path = Some(cache_path);

    Ok(skill)
}

/// Extract a reasonable name from a URL path.
fn extract_name_from_url(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or("remote-skill")
        .trim_end_matches(".md")
        .replace(['-', '_'], " ")
}

/// Fetch content from a URL (sync, uses a temporary tokio runtime).
fn fetch_url_content(url: &str) -> Result<String, SkillError> {
    let url_owned = url.to_string();

    let fetch = async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("opendev-rust/0.1.0")
            .build()
            .map_err(|e| SkillError::NetworkError(e.to_string()))?;

        let resp = client
            .get(&url_owned)
            .send()
            .await
            .map_err(|e| SkillError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(SkillError::NetworkError(format!(
                "HTTP {} for {}",
                resp.status(),
                url_owned
            )));
        }

        resp.text()
            .await
            .map_err(|e| SkillError::NetworkError(e.to_string()))
    };

    // Try to use existing runtime, fall back to creating one
    match tokio::runtime::Handle::try_current() {
        Ok(_handle) => std::thread::scope(|s| {
            s.spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| SkillError::NetworkError(e.to_string()))
                    .and_then(|rt| rt.block_on(fetch))
            })
            .join()
            .unwrap_or_else(|_| Err(SkillError::NetworkError("thread join failed".to_string())))
        }),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| SkillError::NetworkError(e.to_string()))
            .and_then(|rt| rt.block_on(fetch)),
    }
}

/// Load a skill from a local file path.
pub fn load_skill_from_file(path: &Path) -> Result<Skill, SkillError> {
    let content = std::fs::read_to_string(path)?;
    let fallback_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("local-skill")
        .to_string();
    let mut skill = parse_skill(&content, &fallback_name)?;
    skill.cache_path = Some(path.to_path_buf());
    Ok(skill)
}

/// List all cached skills in the skills directory.
pub fn list_cached_skills(cache_dir: Option<&Path>) -> Vec<PathBuf> {
    let skills_dir = cache_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_skills_dir);

    if !skills_dir.exists() {
        return Vec::new();
    }

    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;
