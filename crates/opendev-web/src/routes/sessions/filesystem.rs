//! Filesystem navigation routes: file listing, path verification, directory browsing.

use std::path::Path;

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::error::WebError;
use crate::state::AppState;

/// Query parameters for file listing.
#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    #[serde(default)]
    pub query: String,
}

/// Verify path request.
#[derive(Debug, Deserialize)]
pub struct VerifyPathRequest {
    #[serde(default)]
    pub path: Option<String>,
}

/// Browse directory request.
#[derive(Debug, Deserialize)]
pub struct BrowseDirectoryRequest {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub show_hidden: bool,
}

/// List files in the current session's working directory.
pub(super) async fn list_files(
    State(state): State<AppState>,
    Query(params): Query<ListFilesQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mgr = state.session_manager().await;
    let session = mgr.current_session();

    let working_dir = match session.and_then(|s| s.working_directory.as_deref()) {
        Some(wd) => wd.to_string(),
        None => {
            return Ok(Json(serde_json::json!({"files": []})));
        }
    };

    let wd_path = Path::new(&working_dir);
    if !wd_path.exists() || !wd_path.is_dir() {
        return Ok(Json(serde_json::json!({"files": []})));
    }

    // Directories to always exclude.
    let always_exclude: &[&str] = &[
        ".git",
        ".hg",
        ".svn",
        "node_modules",
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
        ".venv",
        "venv",
        ".DS_Store",
        ".idea",
        ".vscode",
        "target",
        "dist",
        "build",
        "out",
        ".next",
        ".nuxt",
        ".cache",
        ".tox",
        ".nox",
        ".gradle",
        "coverage",
        "htmlcov",
    ];

    let query = params.query.to_lowercase();
    let mut files: Vec<serde_json::Value> = Vec::new();
    let max_files = 100;

    // Walk directory tree (iterative BFS).
    let mut stack = vec![wd_path.to_path_buf()];
    'outer: while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                if !always_exclude.contains(&name.as_ref()) {
                    stack.push(entry.path());
                }
                continue;
            }

            if file_type.is_file() {
                let rel_path = match entry.path().strip_prefix(wd_path) {
                    Ok(p) => p.to_string_lossy().to_string(),
                    Err(_) => continue,
                };

                // Filter by query if provided.
                if !query.is_empty() && !rel_path.to_lowercase().contains(&query) {
                    continue;
                }

                files.push(serde_json::json!({
                    "path": rel_path,
                    "name": name,
                    "is_file": true,
                }));

                if files.len() >= max_files {
                    break 'outer;
                }
            }
        }
    }

    // Sort by path.
    files.sort_by(|a, b| {
        let pa = a["path"].as_str().unwrap_or("");
        let pb = b["path"].as_str().unwrap_or("");
        pa.cmp(pb)
    });

    Ok(Json(serde_json::json!({"files": files})))
}

/// Verify if a directory path exists and is accessible.
pub(super) async fn verify_path(
    State(_state): State<AppState>,
    Json(payload): Json<VerifyPathRequest>,
) -> Json<serde_json::Value> {
    let path_str = payload.path.as_deref().unwrap_or("").trim().to_string();

    if path_str.is_empty() {
        return Json(serde_json::json!({
            "exists": false,
            "is_directory": false,
            "error": "Path cannot be empty",
        }));
    }

    // Expand ~ to home directory.
    let expanded = if path_str.starts_with('~') {
        if let Some(home) = dirs_path_home() {
            path_str.replacen('~', &home, 1)
        } else {
            path_str.clone()
        }
    } else {
        path_str.clone()
    };

    let path = Path::new(&expanded);

    if !path.exists() {
        return Json(serde_json::json!({
            "exists": false,
            "is_directory": false,
            "error": "Path does not exist",
        }));
    }

    if !path.is_dir() {
        return Json(serde_json::json!({
            "exists": true,
            "is_directory": false,
            "error": "Path is not a directory",
        }));
    }

    // Check read access by trying to read_dir.
    if std::fs::read_dir(path).is_err() {
        return Json(serde_json::json!({
            "exists": true,
            "is_directory": true,
            "error": "No read access to directory",
        }));
    }

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    Json(serde_json::json!({
        "exists": true,
        "is_directory": true,
        "path": canonical.to_string_lossy(),
        "error": null,
    }))
}

/// Browse directories at a given path for the workspace picker.
pub(super) async fn browse_directory(
    State(_state): State<AppState>,
    Json(payload): Json<BrowseDirectoryRequest>,
) -> Json<serde_json::Value> {
    let raw = payload.path.trim().to_string();

    let target = if raw.is_empty() {
        // Default to home directory.
        match dirs_path_home() {
            Some(home) => std::path::PathBuf::from(home),
            None => std::path::PathBuf::from("/"),
        }
    } else {
        let expanded = if raw.starts_with('~') {
            if let Some(home) = dirs_path_home() {
                raw.replacen('~', &home, 1)
            } else {
                raw.clone()
            }
        } else {
            raw.clone()
        };
        std::path::PathBuf::from(expanded)
    };

    let target = target.canonicalize().unwrap_or_else(|_| target.clone());

    if !target.exists() {
        return Json(serde_json::json!({
            "current_path": target.to_string_lossy(),
            "parent_path": target.parent().map(|p| p.to_string_lossy().to_string()),
            "directories": [],
            "error": "Path does not exist",
        }));
    }

    if !target.is_dir() {
        return Json(serde_json::json!({
            "current_path": target.to_string_lossy(),
            "parent_path": target.parent().map(|p| p.to_string_lossy().to_string()),
            "directories": [],
            "error": "Path is not a directory",
        }));
    }

    let parent_path = if target.parent() != Some(&target) {
        target.parent().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    let entries = match std::fs::read_dir(&target) {
        Ok(e) => e,
        Err(_) => {
            return Json(serde_json::json!({
                "current_path": target.to_string_lossy(),
                "parent_path": parent_path,
                "directories": [],
                "error": "Permission denied reading directory contents",
            }));
        }
    };

    let mut dirs: Vec<serde_json::Value> = Vec::new();
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && !payload.show_hidden {
            continue;
        }
        // Check read access.
        if std::fs::read_dir(entry.path()).is_err() {
            continue;
        }
        dirs.push(serde_json::json!({
            "name": name,
            "path": entry.path().to_string_lossy(),
        }));
    }

    dirs.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("").to_lowercase();
        let nb = b["name"].as_str().unwrap_or("").to_lowercase();
        na.cmp(&nb)
    });

    Json(serde_json::json!({
        "current_path": target.to_string_lossy(),
        "parent_path": parent_path,
        "directories": dirs,
        "error": null,
    }))
}

/// Helper: get the home directory path as a String.
fn dirs_path_home() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
}
