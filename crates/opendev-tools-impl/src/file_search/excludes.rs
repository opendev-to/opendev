//! Default search exclusion patterns and ignore file generation.
//!
//! Provides directory and glob-based exclusion lists for file search,
//! plus a cached ignore file for ripgrep integration.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Default directories to exclude from search and file listing.
/// Covers 20+ programming languages and ecosystems.
/// Ripgrep already respects `.gitignore`, but these act as a safety net
/// for repos without gitignore or for directories not tracked by git.
pub const DEFAULT_SEARCH_EXCLUDES: &[&str] = &[
    // Package/Dependency Directories
    "node_modules",
    "bower_components",
    "jspm_packages",
    "vendor",
    "Pods",
    ".bundle",
    "packages",
    ".pub-cache",
    ".pub",
    "deps",
    ".nuget",
    ".m2",
    // Virtual Environments
    ".venv",
    "venv",
    ".virtualenvs",
    ".conda",
    // Build Output Directories
    "build",
    "dist",
    "out",
    "target",
    "bin",
    "obj",
    "lib",
    "_build",
    "ebin",
    "dist-newstyle",
    ".build",
    "DerivedData",
    "CMakeFiles",
    ".cmake",
    // Framework-Specific Build
    ".next",
    ".nuxt",
    ".angular",
    ".svelte-kit",
    ".vuepress",
    ".gatsby-cache",
    ".parcel-cache",
    ".turbo",
    "dist_electron",
    // Cache Directories
    ".cache",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".hypothesis",
    ".tox",
    ".nox",
    ".eslintcache",
    ".stylelintcache",
    ".gradle",
    ".dart_tool",
    ".mix",
    ".cpcache",
    ".lsp",
    // IDE/Editor Directories
    ".idea",
    ".vscode",
    ".vscode-test",
    ".vs",
    ".metadata",
    ".settings",
    "xcuserdata",
    ".netbeans",
    // Version Control
    ".git",
    ".svn",
    ".hg",
    // Coverage/Testing Output
    "coverage",
    "htmlcov",
    ".nyc_output",
    // Language-Specific Metadata
    ".eggs",
    ".Rproj.user",
    ".julia",
    "_opam",
    ".cabal-sandbox",
    ".stack-work",
    "blib",
];

/// File glob patterns to exclude (matched by extension/suffix).
pub const DEFAULT_SEARCH_EXCLUDE_GLOBS: &[&str] = &[
    "*.min.js",
    "*.min.css",
    "*.bundle.js",
    "*.chunk.js",
    "*.map",
    "*.pyc",
    "*.pyo",
    "*.class",
    "*.o",
    "*.so",
    "*.dylib",
    "*.dll",
    "*.exe",
    "*.beam",
    "*.hi",
    "*.dyn_hi",
    "*.dyn_o",
    "*.egg-info",
];

/// Returns the path to a cached ignore file containing default exclusions.
/// The file is created once on first call and reused for all subsequent searches.
pub fn default_ignore_file() -> Option<&'static PathBuf> {
    static IGNORE_FILE: OnceLock<Option<PathBuf>> = OnceLock::new();
    IGNORE_FILE
        .get_or_init(|| {
            let mut content = String::new();
            for dir in DEFAULT_SEARCH_EXCLUDES {
                content.push_str(dir);
                content.push('/');
                content.push('\n');
            }
            for glob_pat in DEFAULT_SEARCH_EXCLUDE_GLOBS {
                content.push_str(glob_pat);
                content.push('\n');
            }
            // Write to a temp file that persists for the process lifetime
            let path = std::env::temp_dir().join(format!(
                "opendev-search-excludes-{}.ignore",
                uuid::Uuid::new_v4()
            ));
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&path).ok()?;
            use std::io::Write;
            f.write_all(content.as_bytes()).ok()?;
            Some(path)
        })
        .as_ref()
}
