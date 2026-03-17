//! File and URL discovery helpers for the skill system.
//!
//! Handles scanning directories for markdown skill files, detecting skill
//! sources, discovering companion files, fetching remote skills via URL,
//! and cache staleness checking.

use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use super::metadata::{self, CompanionFile, LoadedSkill, SkillSource};

// ============================================================================
// File discovery
// ============================================================================

/// Detect the source type of a skill directory.
pub(super) fn detect_source(skill_dir: &Path) -> SkillSource {
    if let Some(home) = dirs::home_dir() {
        // Check if the path is under the user home directory's .opendev/skills, .claude/skills,
        // or .agents/skills.
        for subdir in &[".opendev", ".claude", ".agents"] {
            let global_dir = home.join(subdir).join("skills");
            if skill_dir.starts_with(&global_dir) {
                return SkillSource::UserGlobal;
            }
        }
    }
    SkillSource::Project
}

/// Recursively find all `.md` files in a directory.
pub(super) fn glob_md_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    collect_md_files(dir, &mut results)?;
    Ok(results)
}

fn collect_md_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

// ============================================================================
// Companion files
// ============================================================================

/// Maximum number of companion files to discover per skill.
const MAX_COMPANION_FILES: usize = 10;

/// Discover companion files alongside a directory-style skill.
///
/// If the skill file is in a subdirectory (e.g. `skills/testing/SKILL.md`),
/// discovers up to [`MAX_COMPANION_FILES`] sibling files, excluding the skill
/// file itself and `.git` directories.
pub(super) fn discover_companion_files(skill_path: &Path) -> Vec<CompanionFile> {
    let skill_dir = match skill_path.parent() {
        Some(d) => d,
        None => return vec![],
    };

    // Only discover companions for directory-style skills (file inside a subdir),
    // not for flat skills sitting directly in the skills root.
    let skill_filename = skill_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    // Heuristic: if the file is named SKILL.md or is inside a subdir that isn't
    // the top-level skills dir, it's a directory-style skill.
    // We collect siblings regardless — even flat skills could have companions
    // if they happen to be in a subdirectory.
    let mut files = Vec::new();
    collect_companion_files(skill_dir, skill_dir, skill_filename, &mut files);
    files.truncate(MAX_COMPANION_FILES);
    files
}

fn collect_companion_files(
    base_dir: &Path,
    dir: &Path,
    exclude_filename: &str,
    out: &mut Vec<metadata::CompanionFile>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if out.len() >= MAX_COMPANION_FILES {
            return;
        }

        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip .git directories.
        if name_str == ".git" {
            continue;
        }

        if path.is_dir() {
            collect_companion_files(base_dir, &path, "", out);
        } else {
            // Skip the skill file itself.
            if dir == base_dir && name_str == exclude_filename {
                continue;
            }

            let relative = path
                .strip_prefix(base_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| name_str.to_string());

            out.push(metadata::CompanionFile {
                path: path.clone(),
                relative_path: relative,
            });
        }
    }
}

// ============================================================================
// URL Skill Discovery
// ============================================================================

/// Timeout for HTTP fetches in seconds.
const URL_FETCH_TIMEOUT_SECS: u64 = 10;

/// Maximum size of downloaded skill content in bytes (1 MB).
const MAX_SKILL_DOWNLOAD_BYTES: usize = 1_000_000;

/// Fetch a URL and return its body as a string.
///
/// Uses `curl` via `std::process::Command` to avoid async runtime conflicts
/// (same approach as remote instructions).
pub(super) fn fetch_url(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sSfL",
            "--max-time",
            &URL_FETCH_TIMEOUT_SECS.to_string(),
            url,
        ])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed for {url}: {stderr}"));
    }

    let body = String::from_utf8_lossy(&output.stdout);
    if body.len() > MAX_SKILL_DOWNLOAD_BYTES {
        return Err(format!(
            "response too large ({} bytes, max {})",
            body.len(),
            MAX_SKILL_DOWNLOAD_BYTES
        ));
    }

    Ok(body.into_owned())
}

/// Pull skills from a remote URL.
///
/// Fetches `index.json` from the URL, downloads listed skill files to a
/// local cache directory, and returns the list of skill directories.
///
/// ## Index Format
/// ```json
/// {
///   "skills": [
///     { "name": "my-skill", "files": ["SKILL.md", "helper.py"] }
///   ]
/// }
/// ```
pub(super) fn pull_url_skills(base_url: &str) -> Result<Vec<PathBuf>, String> {
    let base = if base_url.ends_with('/') {
        base_url.to_string()
    } else {
        format!("{base_url}/")
    };

    // Determine cache directory
    let cache_dir = dirs::cache_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("opendev")
        .join("skills-cache");

    // Fetch index.json
    let index_url = format!("{base}index.json");
    let index_body = fetch_url(&index_url)?;

    let index: serde_json::Value =
        serde_json::from_str(&index_body).map_err(|e| format!("invalid index.json: {e}"))?;

    let skill_entries = index
        .get("skills")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "index.json missing 'skills' array".to_string())?;

    let mut result_dirs = Vec::new();

    for entry in skill_entries {
        let name = match entry.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };

        let files = match entry.get("files").and_then(|v| v.as_array()) {
            Some(f) => f,
            None => continue,
        };

        let skill_dir = cache_dir.join(name);

        // Download each file (skip if already cached)
        for file_val in files {
            let file_name = match file_val.as_str() {
                Some(f) => f,
                None => continue,
            };

            let dest = skill_dir.join(file_name);
            if dest.exists() {
                continue; // Already cached
            }

            let file_url = format!("{base}{name}/{file_name}");
            match fetch_url(&file_url) {
                Ok(content) => {
                    if let Some(parent) = dest.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(&dest, &content) {
                        warn!(
                            file = %dest.display(),
                            error = %e,
                            "Failed to write cached skill file"
                        );
                    } else {
                        debug!(
                            file = %dest.display(),
                            url = file_url,
                            "Downloaded skill file"
                        );
                    }
                }
                Err(e) => {
                    warn!(url = file_url, error = %e, "Failed to download skill file");
                }
            }
        }

        // Only include the directory if it has at least one .md file
        if skill_dir.exists()
            && std::fs::read_dir(&skill_dir)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .any(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                })
                .unwrap_or(false)
        {
            result_dirs.push(skill_dir);
        }
    }

    debug!(
        url = base_url,
        count = result_dirs.len(),
        "Pulled skills from URL"
    );

    Ok(result_dirs)
}

// ============================================================================
// Cache staleness
// ============================================================================

/// Check if a cached skill's file has been modified since it was cached.
///
/// Returns `true` if the file's current mtime is newer than the cached mtime,
/// indicating the cache should be invalidated. Builtin skills (no path) are
/// never stale.
pub(super) fn is_cache_stale(skill: &LoadedSkill) -> bool {
    let path = match &skill.metadata.path {
        Some(p) => p,
        None => return false, // Builtins never stale
    };

    let cached_mtime = match skill.cached_mtime {
        Some(t) => t,
        None => return false, // No mtime recorded — can't check
    };

    match std::fs::metadata(path) {
        Ok(meta) => meta
            .modified()
            .map(|current| current > cached_mtime)
            .unwrap_or(false),
        Err(_) => false, // File gone — keep cache, let load fail if re-invoked
    }
}

#[cfg(test)]
mod tests {
    use super::super::metadata::SkillMetadata;
    use super::*;

    // ---- URL fetching ----

    #[test]
    fn test_fetch_url_invalid_command() {
        // Unreachable URL should return error
        let result = fetch_url("https://192.0.2.1/nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_pull_url_skills_invalid_url() {
        let result = pull_url_skills("https://192.0.2.1/nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("curl failed"));
    }

    #[test]
    fn test_skill_source_url_display() {
        let source = SkillSource::Url("https://example.com/skills".to_string());
        assert_eq!(source.to_string(), "url:https://example.com/skills");
    }

    // ---- Cache invalidation via mtime ----

    #[test]
    fn test_is_cache_stale_builtin_never_stale() {
        let skill = LoadedSkill {
            metadata: SkillMetadata {
                name: "commit".to_string(),
                description: "Builtin commit".to_string(),
                namespace: "default".to_string(),
                path: None,
                source: SkillSource::Builtin,
                model: None,
                agent: None,
            },
            content: "content".to_string(),
            companion_files: vec![],
            cached_mtime: None,
        };
        assert!(!is_cache_stale(&skill));
    }

    #[test]
    fn test_is_cache_stale_no_mtime_not_stale() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("skill.md");
        std::fs::write(&file, "---\nname: test\ndescription: t\n---\ncontent").unwrap();

        let skill = LoadedSkill {
            metadata: SkillMetadata {
                name: "test".to_string(),
                description: "t".to_string(),
                namespace: "default".to_string(),
                path: Some(file),
                source: SkillSource::Project,
                model: None,
                agent: None,
            },
            content: "content".to_string(),
            companion_files: vec![],
            cached_mtime: None, // No mtime recorded
        };
        assert!(!is_cache_stale(&skill));
    }

    #[test]
    fn test_is_cache_stale_unmodified_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("skill.md");
        std::fs::write(&file, "---\nname: test\ndescription: t\n---\ncontent").unwrap();

        let mtime = std::fs::metadata(&file).unwrap().modified().unwrap();

        let skill = LoadedSkill {
            metadata: SkillMetadata {
                name: "test".to_string(),
                description: "t".to_string(),
                namespace: "default".to_string(),
                path: Some(file),
                source: SkillSource::Project,
                model: None,
                agent: None,
            },
            content: "content".to_string(),
            companion_files: vec![],
            cached_mtime: Some(mtime),
        };
        assert!(!is_cache_stale(&skill));
    }

    #[test]
    fn test_is_cache_stale_modified_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("skill.md");
        std::fs::write(&file, "---\nname: test\ndescription: t\n---\noriginal").unwrap();

        // Record an old mtime (1 second in the past).
        let old_mtime = std::time::SystemTime::now() - std::time::Duration::from_secs(2);

        let skill = LoadedSkill {
            metadata: SkillMetadata {
                name: "test".to_string(),
                description: "t".to_string(),
                namespace: "default".to_string(),
                path: Some(file),
                source: SkillSource::Project,
                model: None,
                agent: None,
            },
            content: "original".to_string(),
            companion_files: vec![],
            cached_mtime: Some(old_mtime),
        };

        // File was written "now", cached mtime is 2s in the past → stale.
        assert!(is_cache_stale(&skill));
    }

    #[test]
    fn test_is_cache_stale_deleted_file() {
        let skill = LoadedSkill {
            metadata: SkillMetadata {
                name: "gone".to_string(),
                description: "t".to_string(),
                namespace: "default".to_string(),
                path: Some(std::path::PathBuf::from("/nonexistent/skill.md")),
                source: SkillSource::Project,
                model: None,
                agent: None,
            },
            content: "content".to_string(),
            companion_files: vec![],
            cached_mtime: Some(std::time::SystemTime::now()),
        };
        // File doesn't exist → not stale (keep cache).
        assert!(!is_cache_stale(&skill));
    }
}
