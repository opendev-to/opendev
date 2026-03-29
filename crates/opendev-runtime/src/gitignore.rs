//! GitIgnore parser — filter files based on .gitignore patterns.
//!
//! Parses `.gitignore` files from a root directory and all subdirectories,
//! supporting nested gitignore overrides. Always ignores common directories
//! (`.git`, `node_modules`, `__pycache__`, etc.) regardless of patterns.

use std::path::{Path, PathBuf};

use tracing::debug;

/// Directories always ignored regardless of `.gitignore` contents.
pub const ALWAYS_IGNORE_DIRS: &[&str] = &[
    // Version control
    ".git",
    ".hg",
    ".svn",
    ".bzr",
    "_darcs",
    ".fossil",
    // OS generated
    ".DS_Store",
    ".Spotlight-V100",
    ".Trashes",
    "Thumbs.db",
    "desktop.ini",
    "$RECYCLE.BIN",
    // Python caches
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".pytype",
    ".pyre",
    ".hypothesis",
    ".tox",
    ".nox",
    "cython_debug",
    ".eggs",
    // Node/JS caches
    "node_modules",
    ".npm",
    ".yarn",
    ".pnpm-store",
    ".next",
    ".nuxt",
    ".output",
    ".svelte-kit",
    ".angular",
    ".parcel-cache",
    ".turbo",
    // IDE/Editor
    ".idea",
    ".vscode",
    ".vs",
    ".settings",
    // Java/Kotlin
    ".gradle",
    // Elixir
    "_build",
    ".elixir_ls",
    // iOS
    "Pods",
    "DerivedData",
    "xcuserdata",
    // Ruby
    ".bundle",
    // Virtual environments
    ".venv",
    "venv",
    // Misc caches
    ".cache",
    ".sass-cache",
    ".eslintcache",
    ".stylelintcache",
    ".tmp",
    ".temp",
    "tmp",
    "temp",
    // Rust
    "target",
];

/// A parsed `.gitignore` pattern.
#[derive(Debug, Clone)]
struct GitIgnorePattern {
    /// The raw pattern string.
    pattern: String,
    /// Whether this is a negation pattern (starts with `!`).
    negated: bool,
    /// Whether this only matches directories (ends with `/`).
    dir_only: bool,
}

/// A `.gitignore` spec loaded from a specific directory.
#[derive(Debug, Clone)]
struct GitIgnoreSpec {
    /// Directory where this `.gitignore` was found.
    base_dir: PathBuf,
    /// Parsed patterns from the file.
    patterns: Vec<GitIgnorePattern>,
}

/// GitIgnore parser that supports nested `.gitignore` files.
pub struct GitIgnoreParser {
    root_dir: PathBuf,
    specs: Vec<GitIgnoreSpec>,
}

impl GitIgnoreParser {
    /// Create a new parser rooted at the given directory.
    pub fn new(root_dir: &Path) -> Self {
        let root_dir = root_dir
            .canonicalize()
            .unwrap_or_else(|_| root_dir.to_path_buf());
        let mut parser = Self {
            root_dir,
            specs: Vec::new(),
        };
        parser.load_gitignore_files();
        parser
    }

    /// Check whether a path should be ignored.
    pub fn is_ignored(&self, path: &Path) -> bool {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root_dir.join(path)
        };

        let rel = match abs_path.strip_prefix(&self.root_dir) {
            Ok(r) => r,
            Err(_) => return false,
        };

        // Check always-ignored directories
        for component in rel.components() {
            let s = component.as_os_str().to_string_lossy();
            if ALWAYS_IGNORE_DIRS.contains(&s.as_ref()) {
                return true;
            }
        }

        // Check gitignore patterns
        let mut ignored = false;
        for spec in &self.specs {
            // Only apply spec if path is under spec's base dir
            let spec_rel = match abs_path.strip_prefix(&spec.base_dir) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let match_str = spec_rel.to_string_lossy().replace('\\', "/");
            let is_dir = abs_path.is_dir();

            for pat in &spec.patterns {
                if pat.dir_only && !is_dir {
                    continue;
                }

                if matches_pattern(&pat.pattern, &match_str) {
                    ignored = !pat.negated;
                }
            }
        }

        ignored
    }

    /// Check if a directory name is in the always-ignore list.
    pub fn is_always_ignored(name: &str) -> bool {
        ALWAYS_IGNORE_DIRS.contains(&name)
    }

    fn load_gitignore_files(&mut self) {
        // Load root .gitignore
        let root_gitignore = self.root_dir.join(".gitignore");
        if root_gitignore.exists()
            && let Some(spec) = self.parse_gitignore(&root_gitignore, &self.root_dir.clone())
        {
            self.specs.push(spec);
        }

        // Walk subdirectories
        self.walk_for_gitignores(&self.root_dir.clone());
    }

    fn walk_for_gitignores(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if ALWAYS_IGNORE_DIRS.contains(&name.as_str()) {
                continue;
            }

            let gitignore = path.join(".gitignore");
            if gitignore.exists()
                && let Some(spec) = self.parse_gitignore(&gitignore, &path)
            {
                self.specs.push(spec);
            }

            self.walk_for_gitignores(&path);
        }
    }

    fn parse_gitignore(&self, gitignore_path: &Path, base_dir: &Path) -> Option<GitIgnoreSpec> {
        let content = std::fs::read_to_string(gitignore_path).ok()?;
        let mut patterns = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (pattern, negated) = if let Some(rest) = trimmed.strip_prefix('!') {
                (rest.to_string(), true)
            } else {
                (trimmed.to_string(), false)
            };

            let dir_only = pattern.ends_with('/');
            let pattern = if dir_only {
                pattern.trim_end_matches('/').to_string()
            } else {
                pattern
            };

            patterns.push(GitIgnorePattern {
                pattern,
                negated,
                dir_only,
            });
        }

        if patterns.is_empty() {
            debug!("No patterns in {}", gitignore_path.display());
            return None;
        }

        Some(GitIgnoreSpec {
            base_dir: base_dir.to_path_buf(),
            patterns,
        })
    }
}

/// Simple glob pattern matching (supports `*`, `**`, `?`).
fn matches_pattern(pattern: &str, path: &str) -> bool {
    // Handle patterns starting with `/` (root-relative)
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);

    if pattern.contains("**") {
        // ** matches any number of directories
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');
            if prefix.is_empty() && suffix.is_empty() {
                return true;
            }
            if prefix.is_empty() {
                return path.ends_with(suffix) || simple_match(suffix, path);
            }
            if suffix.is_empty() {
                return path.starts_with(prefix) || simple_match(prefix, path);
            }
            // Check if path starts with prefix and ends with suffix
            return path.contains(prefix) && path.contains(suffix);
        }
    }

    // If pattern has no slash, it matches any file with that name
    if !pattern.contains('/') {
        // Match against the last component
        let file_name = path.rsplit('/').next().unwrap_or(path);
        return simple_match(pattern, file_name) || simple_match(pattern, path);
    }

    simple_match(pattern, path)
}

/// Simple wildcard matching (`*` matches anything except `/`, `?` matches one char).
fn simple_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    simple_match_impl(&p, &t)
}

fn simple_match_impl(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    if pattern[0] == '*' {
        // Skip consecutive stars
        let mut i = 0;
        while i < pattern.len() && pattern[i] == '*' {
            i += 1;
        }
        if i >= pattern.len() {
            return true;
        }
        for j in 0..=text.len() {
            if simple_match_impl(&pattern[i..], &text[j..]) {
                return true;
            }
        }
        return false;
    }
    if text.is_empty() {
        return false;
    }
    if pattern[0] == '?' || pattern[0] == text[0] {
        return simple_match_impl(&pattern[1..], &text[1..]);
    }
    false
}

impl std::fmt::Debug for GitIgnoreParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitIgnoreParser")
            .field("root_dir", &self.root_dir)
            .field("specs_count", &self.specs.len())
            .finish()
    }
}

#[cfg(test)]
#[path = "gitignore_tests.rs"]
mod tests;
