//! Fast file search with gitignore awareness.
//!
//! Mirrors the Python `FileFinder` and `FileSizeFormatter` classes, using the
//! `ignore` crate for `.gitignore`-aware directory walking.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// ── Constants ──────────────────────────────────────────────────────

/// How long cached file lists remain valid.
const CACHE_TTL: Duration = Duration::from_secs(30);

/// Maximum number of entries to keep in the cache.
const MAX_CACHE_SIZE: usize = 5000;

/// Directories that are always excluded, even when no `.gitignore` exists.
const ALWAYS_EXCLUDE: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    "node_modules",
    ".venv",
    "venv",
    ".tox",
    ".nox",
    ".next",
    ".nuxt",
    ".idea",
    ".vscode",
    ".DS_Store",
    ".cache",
    ".eggs",
    ".gradle",
    "Pods",
    ".bundle",
    ".sass-cache",
    ".tmp",
    "tmp",
    "temp",
];

/// Directories that are *likely* build output (excluded when no `.gitignore`).
const LIKELY_EXCLUDE: &[&str] = &[
    "dist",
    "build",
    "out",
    "bin",
    "obj",
    "target",
    "coverage",
    "htmlcov",
    "vendor",
    "packages",
    "bower_components",
];

// ── FileFinder ─────────────────────────────────────────────────────

/// Cached, gitignore-aware file finder.
pub struct FileFinder {
    working_dir: PathBuf,
    /// Cached entries: each is a relative path (lowered for matching) + original relative path.
    cache: RefCell<Vec<(String, PathBuf)>>,
    cache_time: RefCell<Option<Instant>>,
    exclude_set: HashSet<&'static str>,
    has_gitignore: bool,
}

impl FileFinder {
    /// Create a new `FileFinder` rooted at `working_dir`.
    pub fn new(working_dir: PathBuf) -> Self {
        let has_gitignore = working_dir.join(".gitignore").exists();
        let mut exclude_set: HashSet<&str> = ALWAYS_EXCLUDE.iter().copied().collect();
        if !has_gitignore {
            exclude_set.extend(LIKELY_EXCLUDE.iter());
        }
        Self {
            working_dir,
            cache: RefCell::new(Vec::new()),
            cache_time: RefCell::new(None),
            exclude_set,
            has_gitignore,
        }
    }

    /// The working directory this finder searches in.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// Find files matching `query` (substring match, case-insensitive).
    ///
    /// Returns relative paths sorted by path length then alphabetically.
    pub fn find_files(&self, query: &str, max_results: usize) -> Vec<PathBuf> {
        self.ensure_cache();
        let query_lower = query.to_lowercase();
        let cache = self.cache.borrow();

        cache
            .iter()
            .filter(|(lower, _)| query_lower.is_empty() || lower.contains(&query_lower))
            .map(|(_, p)| p.clone())
            .take(max_results)
            .collect()
    }

    /// Force a fresh directory scan (ignoring any cache).
    pub fn invalidate_cache(&self) {
        self.cache.borrow_mut().clear();
        *self.cache_time.borrow_mut() = None;
    }

    // ── Internal ───────────────────────────────────────────────────

    fn is_cache_valid(&self) -> bool {
        self.cache_time
            .borrow()
            .map(|t| t.elapsed() < CACHE_TTL && !self.cache.borrow().is_empty())
            .unwrap_or(false)
    }

    /// Populate the cache if it is stale or empty.
    fn ensure_cache(&self) {
        if self.is_cache_valid() {
            return;
        }

        let mut entries: Vec<(String, PathBuf)> = Vec::new();

        if self.has_gitignore {
            self.walk_with_ignore(&mut entries);
        } else {
            self.walk_manual(&self.working_dir, &mut entries);
        }

        // Sort by path length then alphabetically
        entries.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));

        *self.cache.borrow_mut() = entries;
        *self.cache_time.borrow_mut() = Some(Instant::now());
    }

    fn walk_with_ignore(&self, entries: &mut Vec<(String, PathBuf)>) {
        use ignore::WalkBuilder;

        let walker = WalkBuilder::new(&self.working_dir)
            .hidden(true) // respect hidden files
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .max_depth(Some(10))
            .build();

        for result in walker {
            if entries.len() >= MAX_CACHE_SIZE {
                break;
            }
            if let Ok(entry) = result {
                let path = entry.path();
                // Skip the root itself
                if path == self.working_dir {
                    continue;
                }
                if let Ok(rel) = path.strip_prefix(&self.working_dir) {
                    let rel_path = rel.to_path_buf();
                    let lower = rel_path.to_string_lossy().to_lowercase();
                    entries.push((lower, rel_path));
                }
            }
        }
    }

    fn walk_manual(&self, dir: &Path, entries: &mut Vec<(String, PathBuf)>) {
        if entries.len() >= MAX_CACHE_SIZE {
            return;
        }
        let read_dir = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };
        for entry in read_dir.flatten() {
            if entries.len() >= MAX_CACHE_SIZE {
                break;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if path.is_dir() {
                if self.exclude_set.contains(name_str.as_ref()) {
                    continue;
                }
                // Add directory entry
                if let Ok(rel) = path.strip_prefix(&self.working_dir) {
                    let rel_path = rel.to_path_buf();
                    let lower = rel_path.to_string_lossy().to_lowercase();
                    entries.push((lower, rel_path));
                }
                self.walk_manual(&path, entries);
            } else if let Ok(rel) = path.strip_prefix(&self.working_dir) {
                let rel_path = rel.to_path_buf();
                let lower = rel_path.to_string_lossy().to_lowercase();
                entries.push((lower, rel_path));
            }
        }
    }
}

// ── FileSizeFormatter ──────────────────────────────────────────────

/// Format a byte count into a human-readable string.
pub fn format_file_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "file_finder_tests.rs"]
mod tests;
