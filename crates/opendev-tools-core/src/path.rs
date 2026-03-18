//! Canonical path resolution for LLM-produced tool parameters.
//!
//! LLMs frequently return incorrect paths — relative paths, redundant basename
//! prefixes (e.g., `myproject/src/main.rs` when cwd is already `myproject`),
//! `./` prefixes, `$HOME` paths, etc. This module provides the single source
//! of truth for resolving such paths.

use std::path::{Component, Path, PathBuf};

/// Expand tilde (`~`) and `$HOME` prefixes in a path string.
///
/// - `~/foo` -> `/home/user/foo`
/// - `$HOME/foo` -> `/home/user/foo`
/// - `~` -> `/home/user`
/// - Other paths are returned as-is.
pub fn expand_home(path: &str) -> String {
    if path == "~" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return format!("{}/{}", home.display(), rest);
    }
    if let Some(rest) = path.strip_prefix("$HOME/")
        && let Some(home) = dirs::home_dir()
    {
        return format!("{}/{}", home.display(), rest);
    }
    if path == "$HOME" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
    }
    path.to_string()
}

/// Strip leading `.` and `./` components from a path, returning the
/// meaningful portion. E.g., `./myproject/src` -> `myproject/src`.
pub fn strip_curdir(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

/// Normalize a path by collapsing `.` and `..` components without touching the filesystem.
///
/// Unlike `canonicalize()`, this works on paths that don't exist yet.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {} // skip `.`
            Component::ParentDir => {
                // Pop the last component if it's a normal component.
                if let Some(last) = components.last()
                    && !matches!(last, Component::RootDir | Component::Prefix(_))
                {
                    components.pop();
                    continue;
                }
                components.push(component);
            }
            _ => components.push(component),
        }
    }

    components.iter().collect()
}

/// Strip hallucinated Docker-style prefixes like `/workspace/` or `/testbed/`.
///
/// If the path starts with a known fake prefix AND the resulting absolute path
/// doesn't exist, rewrites it to be relative to the working directory.
/// If the original absolute path does exist (e.g., there really is a `/workspace/` dir),
/// it is left unchanged.
fn strip_hallucinated_prefix(path_str: &str, working_dir: &Path) -> String {
    for prefix in HALLUCINATED_PREFIXES {
        if let Some(rest) = path_str.strip_prefix(prefix) {
            let original = Path::new(path_str);
            // Only rewrite if the original doesn't exist but the working_dir version does
            // (or the working_dir version's parent exists for new file creation).
            if !original.exists() {
                let candidate = working_dir.join(rest);
                if candidate.exists() || candidate.parent().map(|p| p.is_dir()).unwrap_or(false) {
                    return candidate.to_string_lossy().to_string();
                }
                // Even if candidate doesn't exist, still rewrite — `/workspace/` is almost
                // certainly wrong on a real system.
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    // Also handle bare `/workspace` or `/testbed` (without trailing slash or subpath)
    let bare_prefixes = ["/workspace", "/testbed"];
    for prefix in &bare_prefixes {
        if path_str == *prefix && !Path::new(prefix).exists() {
            return working_dir.to_string_lossy().to_string();
        }
    }
    path_str.to_string()
}

/// Well-known fake prefixes that LLMs hallucinate from Docker training data.
/// When we see these as absolute path prefixes and the real path doesn't exist,
/// we strip them and resolve relative to the actual working directory.
const HALLUCINATED_PREFIXES: &[&str] = &["/workspace/", "/testbed/"];

/// Resolve a user-provided file path against the working directory.
///
/// Handles common LLM mistakes:
/// - `./src/main.rs` -> strips `./` prefix
/// - `~/file.rs` / `$HOME/file.rs` -> expands home directory
/// - `myproject/main.rs` when cwd is `/home/user/myproject` -> `/home/user/myproject/main.rs`
///   (detects and strips redundant basename prefix)
/// - Absolute paths with doubled project name
/// - `/workspace/foo` or `/testbed/foo` -> `{working_dir}/foo` (LLM hallucination from Docker)
pub fn resolve_file_path(user_path: &str, working_dir: &Path) -> PathBuf {
    let expanded = expand_home(user_path);
    // Rewrite hallucinated Docker prefixes to working_dir-relative paths
    let expanded = strip_hallucinated_prefix(&expanded, working_dir);
    let path = strip_curdir(Path::new(&expanded));
    let path = normalize_path(&path);
    let path = path.as_path();
    if path.is_absolute() {
        if path.exists() {
            return path.to_path_buf();
        }
        // Check if the path has a redundant component matching the working dir basename.
        // e.g., /home/user/myproject/myproject/src/main.rs -> /home/user/myproject/src/main.rs
        if let Ok(rel) = path.strip_prefix(working_dir)
            && let Some(first) = rel.components().next()
        {
            let first_name = first.as_os_str();
            if working_dir
                .file_name()
                .map(|n| n == first_name)
                .unwrap_or(false)
            {
                let fixed = working_dir.join(rel.strip_prefix(first_name).unwrap_or(rel));
                // Accept if the file exists OR its parent directory exists
                // (supports new file creation with redundant prefix)
                if fixed.exists() || fixed.parent().map(|p| p.is_dir()).unwrap_or(false) {
                    return fixed;
                }
            }
        }
        path.to_path_buf()
    } else {
        let joined = normalize_path(&working_dir.join(path));
        if joined.exists() {
            return joined;
        }
        // Check if first component matches working dir basename (redundant prefix)
        let mut components = path.components();
        if let Some(first) = components.next() {
            let first_name = first.as_os_str();
            if working_dir
                .file_name()
                .map(|n| n == first_name)
                .unwrap_or(false)
            {
                let rest: PathBuf = components.collect();
                if !rest.as_os_str().is_empty() {
                    let fixed = normalize_path(&working_dir.join(&rest));
                    if fixed.exists() || fixed.parent().map(|p| p.is_dir()).unwrap_or(false) {
                        return fixed;
                    }
                }
            }
        }
        joined
    }
}

/// Resolve a user-provided directory path against the working directory.
///
/// Same as [`resolve_file_path`] but optimized for directory paths. If a relative
/// path doesn't exist when joined with working_dir, checks if stripping a redundant
/// leading directory component (matching the working dir's basename) helps.
pub fn resolve_dir_path(user_path: &str, working_dir: &Path) -> PathBuf {
    let expanded = expand_home(user_path);
    // Rewrite hallucinated Docker prefixes to working_dir-relative paths
    let expanded = strip_hallucinated_prefix(&expanded, working_dir);
    let path = strip_curdir(Path::new(&expanded));
    let path = normalize_path(&path);
    let path = path.as_path();
    if path.is_absolute() {
        if path.is_dir() {
            return path.to_path_buf();
        }
        // Check if the path has a redundant component matching the working dir basename.
        if let Ok(rel) = path.strip_prefix(working_dir)
            && let Some(first) = rel.components().next()
        {
            let first_name = first.as_os_str();
            if working_dir
                .file_name()
                .map(|n| n == first_name)
                .unwrap_or(false)
            {
                let fixed = working_dir.join(rel.strip_prefix(first_name).unwrap_or(rel));
                if fixed.is_dir() || fixed.parent().map(|p| p.is_dir()).unwrap_or(false) {
                    return fixed;
                }
            }
        }
        // Absolute path doesn't exist as a directory — check if it matches
        // the working directory or is a parent prefix of it.
        if working_dir.starts_with(path) || working_dir == path {
            working_dir.to_path_buf()
        } else {
            path.to_path_buf()
        }
    } else {
        let joined = normalize_path(&working_dir.join(path));
        if joined.is_dir() {
            return joined;
        }
        // Check if first component matches working dir basename (redundant prefix)
        let mut components = path.components();
        if let Some(first) = components.next() {
            let first_name = first.as_os_str();
            if working_dir
                .file_name()
                .map(|n| n == first_name)
                .unwrap_or(false)
            {
                let rest: PathBuf = components.collect();
                if rest.as_os_str().is_empty() {
                    // Single component matching basename — fall back to cwd
                    return working_dir.to_path_buf();
                }
                let fixed = working_dir.join(&rest);
                if fixed.is_dir() || fixed.parent().map(|p| p.is_dir()).unwrap_or(false) {
                    return fixed;
                }
            }
        }
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ---- expand_home ----

    #[test]
    fn test_expand_home_tilde_prefix() {
        let result = expand_home("~/projects/foo");
        assert!(!result.starts_with("~"));
        assert!(result.ends_with("/projects/foo"));
    }

    #[test]
    fn test_expand_home_tilde_only() {
        let result = expand_home("~");
        assert!(!result.starts_with("~"));
        assert!(result.starts_with('/'));
    }

    #[test]
    fn test_expand_home_dollar_home_prefix() {
        let result = expand_home("$HOME/Documents");
        assert!(!result.starts_with("$HOME"));
        assert!(result.ends_with("/Documents"));
    }

    #[test]
    fn test_expand_home_dollar_home_only() {
        let result = expand_home("$HOME");
        assert!(!result.contains("$HOME"));
        assert!(result.starts_with('/'));
    }

    #[test]
    fn test_expand_home_no_expansion() {
        assert_eq!(expand_home("/absolute/path"), "/absolute/path");
        assert_eq!(expand_home("relative/path"), "relative/path");
        assert_eq!(expand_home("~not-a-tilde"), "~not-a-tilde");
    }

    // ---- normalize_path ----

    #[test]
    fn test_normalize_path_collapses_dotdot() {
        let result = normalize_path(Path::new("/home/user/project/../../../etc/passwd"));
        assert_eq!(result, PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn test_normalize_path_collapses_dot() {
        let result = normalize_path(Path::new("/home/user/./project/./src"));
        assert_eq!(result, PathBuf::from("/home/user/project/src"));
    }

    #[test]
    fn test_normalize_path_preserves_root() {
        let result = normalize_path(Path::new("/../../etc"));
        assert_eq!(result, PathBuf::from("/etc"));
    }

    // ---- resolve_file_path ----

    #[test]
    fn test_resolve_file_path_redundant_prefix() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}").unwrap();

        let result = resolve_file_path("myproject/main.rs", &project);
        assert_eq!(result, project.join("main.rs"));
    }

    #[test]
    fn test_resolve_file_path_valid_relative() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "").unwrap();

        let result = resolve_file_path("src/lib.rs", &project);
        assert_eq!(result, project.join("src/lib.rs"));
    }

    #[test]
    fn test_resolve_file_path_absolute_redundant() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("main.rs"), "").unwrap();

        let wrong_path = project.join("myproject").join("main.rs");
        let result = resolve_file_path(wrong_path.to_str().unwrap(), &project);
        assert_eq!(result, project.join("main.rs"));
    }

    #[test]
    fn test_resolve_file_path_dot_slash_redundant() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}").unwrap();

        let result = resolve_file_path("./myproject/main.rs", &project);
        assert_eq!(result, project.join("main.rs"));
    }

    #[test]
    fn test_resolve_file_path_dot_slash_valid() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "").unwrap();

        let result = resolve_file_path("./src/lib.rs", &project);
        assert_eq!(result, project.join("src/lib.rs"));
    }

    #[test]
    fn test_resolve_file_path_tilde_redundant() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();

        let result = resolve_file_path("~/nonexistent_dir_xyz/file.rs", &project);
        assert!(
            !result.to_string_lossy().contains('~'),
            "tilde should be expanded: {result:?}"
        );
    }

    // ---- resolve_dir_path ----

    #[test]
    fn test_resolve_dir_path_redundant_basename() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();

        let result = resolve_dir_path("myproject", &project);
        assert_eq!(result, project);
    }

    #[test]
    fn test_resolve_dir_path_valid_subdir() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let subdir = project.join("src");
        fs::create_dir_all(&subdir).unwrap();

        let result = resolve_dir_path("src", &project);
        assert_eq!(result, subdir);
    }

    #[test]
    fn test_resolve_dir_path_absolute_existing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("existing");
        fs::create_dir(&dir).unwrap();

        let result = resolve_dir_path(dir.to_str().unwrap(), tmp.path());
        assert_eq!(result, dir);
    }

    #[test]
    fn test_resolve_dir_path_no_path() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_dir_path("nonexistent", tmp.path());
        assert_eq!(result, tmp.path().join("nonexistent"));
    }

    #[test]
    fn test_resolve_dir_path_absolute_redundant() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();

        let wrong_path = project.join("myproject").join("src");
        let result = resolve_dir_path(wrong_path.to_str().unwrap(), &project);
        assert_eq!(result, src);
    }

    #[test]
    fn test_resolve_dir_path_relative_multi_component() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();

        let result = resolve_dir_path("myproject/src", &project);
        assert_eq!(result, src);
    }

    #[test]
    fn test_resolve_dir_path_absolute_redundant_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();

        let wrong_path = project.join("myproject").join("newdir");
        let result = resolve_dir_path(wrong_path.to_str().unwrap(), &project);
        assert_eq!(result, project.join("newdir"));
    }

    #[test]
    fn test_resolve_dir_path_dot_slash_redundant() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();

        let result = resolve_dir_path("./myproject/src", &project);
        assert_eq!(result, src);
    }

    #[test]
    fn test_resolve_dir_path_tilde_expanded() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();

        let result = resolve_dir_path("~/nonexistent_dir_xyz", &project);
        assert!(
            !result.to_string_lossy().contains('~'),
            "tilde should be expanded: {result:?}"
        );
    }

    // ---- hallucinated prefix tests ----

    #[test]
    fn test_resolve_file_path_workspace_prefix() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();

        // LLM hallucinates /workspace/package.json
        let result = resolve_file_path("/workspace/package.json", &project);
        assert_eq!(result, project.join("package.json"));
    }

    #[test]
    fn test_resolve_file_path_workspace_nested() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.rs"), "fn main() {}").unwrap();

        let result = resolve_file_path("/workspace/src/main.rs", &project);
        assert_eq!(result, project.join("src/main.rs"));
    }

    #[test]
    fn test_resolve_file_path_testbed_prefix() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("main.py"), "").unwrap();

        let result = resolve_file_path("/testbed/main.py", &project);
        assert_eq!(result, project.join("main.py"));
    }

    #[test]
    fn test_resolve_dir_path_workspace_prefix() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        let src = project.join("src");
        fs::create_dir_all(&src).unwrap();

        let result = resolve_dir_path("/workspace/src", &project);
        assert_eq!(result, src);
    }

    #[test]
    fn test_resolve_dir_path_bare_workspace() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();

        // /workspace alone should map to working_dir
        let result = resolve_dir_path("/workspace", &project);
        assert_eq!(result, project);
    }

    #[test]
    fn test_resolve_file_path_workspace_nonexistent_file() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("myproject");
        fs::create_dir(&project).unwrap();

        // Even if the file doesn't exist, /workspace/ should still be rewritten
        let result = resolve_file_path("/workspace/newfile.rs", &project);
        assert_eq!(result, project.join("newfile.rs"));
    }
}
