//! Auto-scout: project structure scanner for Explore subagents.
//!
//! Scans a directory tree and produces a tree-style listing that gives
//! subagents an initial view of the project layout without burning tool calls.

/// Directories to skip when scanning project structure.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "__pycache__",
    "dist",
    "build",
    "vendor",
    ".venv",
    ".vscode",
    ".idea",
    ".next",
    ".cache",
    ".tox",
    "venv",
    "env",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
];

/// Cap on total entries to avoid flooding context.
const MAX_SCAN_ENTRIES: usize = 100;

/// Return a comma-separated list of top-level directory names (excluding noise dirs).
/// Used by the auto-scout prompt to give the LLM an explicit allowlist.
pub(super) fn scan_top_level_dirs(root: &std::path::Path) -> String {
    let mut dirs: Vec<String> = match std::fs::read_dir(root) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| !name.starts_with('.') && !SKIP_DIRS.iter().any(|&s| s == name))
            .collect(),
        Err(_) => return String::new(),
    };
    dirs.sort();
    dirs.iter()
        .map(|d| format!("{d}/"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Scan a directory tree up to `max_depth` levels and return a tree-style
/// string listing. Returns an empty string if the directory cannot be read.
pub(super) fn scan_project_structure(root: &std::path::Path, max_depth: usize) -> String {
    let mut entries: Vec<String> = Vec::new();
    scan_dir(root, "", max_depth, 0, &mut entries);
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from("Project structure:\n");
    for line in &entries {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Recursive helper that collects tree lines into `out`.
fn scan_dir(
    dir: &std::path::Path,
    prefix: &str,
    max_depth: usize,
    depth: usize,
    out: &mut Vec<String>,
) {
    if out.len() >= MAX_SCAN_ENTRIES {
        return;
    }

    let mut children: Vec<std::fs::DirEntry> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };

    // Sort entries: directories first, then alphabetical.
    children.sort_by(|a, b| {
        let a_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        b_dir
            .cmp(&a_dir)
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    let total = children.len();
    for (i, entry) in children.into_iter().enumerate() {
        if out.len() >= MAX_SCAN_ENTRIES {
            out.push(format!("{prefix}... (truncated)"));
            return;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

        if is_dir {
            // Skip excluded directories
            if SKIP_DIRS.iter().any(|&s| s == name_str) {
                continue;
            }
            out.push(format!("{prefix}{connector}{name_str}/"));
            if depth < max_depth {
                let child_prefix = if is_last {
                    format!("{prefix}    ")
                } else {
                    format!("{prefix}│   ")
                };
                scan_dir(&entry.path(), &child_prefix, max_depth, depth + 1, out);
            }
        } else {
            out.push(format!("{prefix}{connector}{name_str}"));
        }
    }
}

#[cfg(test)]
#[path = "scanning_tests.rs"]
mod tests;
