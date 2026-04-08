//! Instruction file discovery and resolution.
//!
//! Discovers project instruction files (AGENTS.md, CLAUDE.md, etc.) by walking
//! the directory hierarchy, and resolves config-specified instruction paths
//! (globs, URLs, ~/paths).
//!
//! Enhanced features:
//! - `.opendev/rules/` directory with conditional frontmatter
//! - Local overrides (`AGENTS.local.md`, `CLAUDE.local.md`)
//! - Managed/enterprise instructions (`/etc/opendev/`)
//! - `@include` directive processing
//! - HTML comment stripping
//! - Instruction exclusion via glob patterns
//! - Additional directories for discovery

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::exclusions::is_excluded;
use super::frontmatter::{parse_frontmatter, strip_html_comments};
use super::include_parser::process_includes;
use super::{InstructionFile, InstructionSource};

/// Instruction file names to search for, in priority order.
const INSTRUCTION_FILENAMES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

/// Local override variants (gitignored, highest priority).
const LOCAL_INSTRUCTION_FILENAMES: &[&str] = &["AGENTS.local.md", "CLAUDE.local.md"];

/// Additional instruction file patterns from other AI tools.
/// These are checked per-directory alongside the standard filenames.
const COMPAT_INSTRUCTION_FILES: &[&str] = &[
    ".cursorrules",                    // Cursor AI (flat file)
    ".github/copilot-instructions.md", // GitHub Copilot
];

/// Max content size per instruction file (50 KB).
const MAX_INSTRUCTION_BYTES: usize = 50 * 1024;

/// Timeout in seconds for fetching remote instructions via HTTP(S).
const REMOTE_INSTRUCTION_TIMEOUT_SECS: u64 = 5;

/// Discover project instruction files by walking up from `working_dir`.
///
/// Searches for `AGENTS.md`, `CLAUDE.md`, local overrides, `.opendev/rules/`,
/// compatibility files, and more. Also checks global and managed locations.
///
/// Files found closer to `working_dir` have higher priority and are listed first.
/// Local overrides are listed after their non-local counterparts for highest priority.
///
/// `exclude_patterns` filters out instruction files matching any glob pattern.
/// `additional_dirs` adds extra directories for discovery.
pub fn discover_instruction_files(
    working_dir: &Path,
    exclude_patterns: &[String],
    additional_dirs: &[PathBuf],
) -> Vec<InstructionFile> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();

    // Walk the primary working directory
    walk_directory_tree(working_dir, exclude_patterns, &mut files, &mut seen);

    // Walk additional directories
    for dir in additional_dirs {
        if dir.is_dir() {
            walk_directory_tree(dir, exclude_patterns, &mut files, &mut seen);
        }
    }

    // Check global config locations
    if let Some(home) = dirs_next::home_dir() {
        let global_paths = [
            home.join(".opendev").join("instructions.md"),
            home.join(".opendev").join("AGENTS.md"),
            home.join(".config").join("opendev").join("AGENTS.md"),
        ];
        for path in &global_paths {
            try_add_instruction(
                path,
                path.parent().unwrap_or(path),
                working_dir,
                exclude_patterns,
                InstructionSource::Discovery,
                &mut files,
                &mut seen,
            );
        }
    }

    // Check managed/enterprise instructions (never excludable)
    let managed_paths = [
        PathBuf::from("/etc/opendev/AGENTS.md"),
        PathBuf::from("/etc/opendev/instructions.md"),
    ];
    for path in &managed_paths {
        try_add_managed_instruction(path, &mut files, &mut seen);
    }

    files
}

/// Walk the directory tree upward from `start_dir`, collecting instruction files.
fn walk_directory_tree(
    start_dir: &Path,
    exclude_patterns: &[String],
    files: &mut Vec<InstructionFile>,
    seen: &mut HashSet<PathBuf>,
) {
    let working_dir = start_dir;
    let mut current = start_dir.to_path_buf();

    loop {
        // Check each instruction filename
        for filename in INSTRUCTION_FILENAMES {
            let candidate = current.join(filename);
            try_add_instruction(
                &candidate,
                &current,
                working_dir,
                exclude_patterns,
                InstructionSource::Discovery,
                files,
                seen,
            );
        }

        // Check local overrides (AGENTS.local.md, CLAUDE.local.md)
        for filename in LOCAL_INSTRUCTION_FILENAMES {
            let candidate = current.join(filename);
            try_add_instruction(
                &candidate,
                &current,
                working_dir,
                exclude_patterns,
                InstructionSource::Local,
                files,
                seen,
            );
        }

        // Check .opendev/instructions.md
        let opendev_instr = current.join(".opendev").join("instructions.md");
        try_add_instruction(
            &opendev_instr,
            &current,
            working_dir,
            exclude_patterns,
            InstructionSource::Discovery,
            files,
            seen,
        );

        // Check .opendev/rules/ directory for individual rule files
        discover_rules_dir(
            &current.join(".opendev").join("rules"),
            &current,
            working_dir,
            exclude_patterns,
            files,
            seen,
        );

        // Check compatibility files from other AI tools (.cursorrules, copilot, etc.)
        for compat_path in COMPAT_INSTRUCTION_FILES {
            let candidate = current.join(compat_path);
            try_add_instruction(
                &candidate,
                &current,
                working_dir,
                exclude_patterns,
                InstructionSource::Discovery,
                files,
                seen,
            );
        }

        // Check .cursor/rules/ directory for individual rule files
        discover_rules_dir(
            &current.join(".cursor").join("rules"),
            &current,
            working_dir,
            exclude_patterns,
            files,
            seen,
        );

        // Stop at git root or filesystem root
        if current.join(".git").exists() {
            break;
        }
        if !current.pop() {
            break;
        }
    }
}

/// Discover rule files in a rules directory (.opendev/rules/ or .cursor/rules/).
///
/// Rule files can have YAML frontmatter with `paths` globs for conditional loading.
/// Files are sorted alphabetically for deterministic ordering.
fn discover_rules_dir(
    rules_dir: &Path,
    dir: &Path,
    working_dir: &Path,
    exclude_patterns: &[String],
    files: &mut Vec<InstructionFile>,
    seen: &mut HashSet<PathBuf>,
) {
    if !rules_dir.is_dir() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(rules_dir) else {
        return;
    };

    let mut rule_files: Vec<_> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            e.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                && (name_str.ends_with(".md")
                    || name_str.ends_with(".txt")
                    || name_str.ends_with(".mdc"))
        })
        .collect();
    rule_files.sort_by_key(|e| e.file_name());

    for entry in rule_files {
        let path = entry.path();
        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };

        if !seen.insert(canonical.clone()) {
            continue;
        }

        if is_excluded(&canonical, working_dir, exclude_patterns) {
            continue;
        }

        let content = match std::fs::read_to_string(&canonical) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }

        let content = truncate_content(content);

        // Parse frontmatter for conditional path globs
        let (frontmatter, remaining_content) = parse_frontmatter(&content);
        let path_globs = frontmatter.and_then(|fm| fm.paths);

        // Strip HTML comments and process includes
        let cleaned = strip_html_comments(&remaining_content);
        let include_dir = canonical.parent().unwrap_or(working_dir);
        let (processed, mut included_files) = process_includes(
            &cleaned,
            include_dir,
            working_dir,
            0,
            seen,
            Some(&canonical),
        );

        // Add included files first (lower priority)
        files.append(&mut included_files);

        let scope = compute_scope(dir, working_dir);
        files.push(InstructionFile {
            scope,
            path: canonical,
            content: processed,
            source: InstructionSource::Rules,
            path_globs,
            included_from: None,
        });
    }
}

/// Try to read an instruction file and add it to the list if it exists.
fn try_add_instruction(
    path: &Path,
    dir: &Path,
    working_dir: &Path,
    exclude_patterns: &[String],
    source: InstructionSource,
    files: &mut Vec<InstructionFile>,
    seen: &mut HashSet<PathBuf>,
) {
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => return, // File doesn't exist
    };
    if !seen.insert(canonical.clone()) {
        return; // Already seen (e.g. symlink or parent overlap)
    }

    if is_excluded(&canonical, working_dir, exclude_patterns) {
        return;
    }

    let content = match std::fs::read_to_string(&canonical) {
        Ok(c) => c,
        Err(_) => return,
    };
    if content.trim().is_empty() {
        return;
    }

    let content = truncate_content(content);

    // Strip HTML comments
    let cleaned = strip_html_comments(&content);

    // Process @include directives
    let include_dir = canonical.parent().unwrap_or(working_dir);
    let (processed, mut included_files) = process_includes(
        &cleaned,
        include_dir,
        working_dir,
        0,
        seen,
        Some(&canonical),
    );

    // Add included files first (lower priority)
    files.append(&mut included_files);

    let scope = match source {
        InstructionSource::Local => "local".to_string(),
        InstructionSource::Managed => "managed".to_string(),
        _ => compute_scope(dir, working_dir),
    };

    files.push(InstructionFile {
        scope,
        path: canonical,
        content: processed,
        source,
        path_globs: None,
        included_from: None,
    });
}

/// Add a managed instruction file (never subject to exclusion).
fn try_add_managed_instruction(
    path: &Path,
    files: &mut Vec<InstructionFile>,
    seen: &mut HashSet<PathBuf>,
) {
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => return,
    };
    if !seen.insert(canonical.clone()) {
        return;
    }

    let content = match std::fs::read_to_string(&canonical) {
        Ok(c) => c,
        Err(_) => return,
    };
    if content.trim().is_empty() {
        return;
    }

    let content = truncate_content(content);
    let cleaned = strip_html_comments(&content);

    files.push(InstructionFile {
        scope: "managed".to_string(),
        path: canonical,
        content: cleaned,
        source: InstructionSource::Managed,
        path_globs: None,
        included_from: None,
    });
}

/// Compute scope label for a directory relative to working_dir.
fn compute_scope(dir: &Path, working_dir: &Path) -> String {
    if dir == working_dir || dir.starts_with(working_dir) {
        "project".to_string()
    } else if dir.to_string_lossy().contains(".opendev")
        || dir.to_string_lossy().contains(".config")
    {
        "global".to_string()
    } else {
        "parent".to_string()
    }
}

/// Truncate content to MAX_INSTRUCTION_BYTES with a note.
fn truncate_content(content: String) -> String {
    if content.len() > MAX_INSTRUCTION_BYTES {
        let truncated = &content[..MAX_INSTRUCTION_BYTES];
        format!(
            "{truncated}\n\n... (truncated, file is {} KB)",
            content.len() / 1024
        )
    } else {
        content
    }
}

/// Resolve config `instructions` entries (file paths, glob patterns, `~/` paths, URLs)
/// into `InstructionFile` entries.
///
/// Each entry can be:
/// - A relative file path (resolved against `working_dir`)
/// - An absolute file path
/// - A glob pattern (e.g. `.cursor/rules/*.md`, `docs/**/*.md`)
/// - A `~/` prefixed path (expanded to home directory)
/// - An `https://` or `http://` URL (fetched with a 5-second timeout)
///
/// Duplicate files (by canonical path) and duplicate URLs are skipped.
pub fn resolve_instruction_paths(patterns: &[String], working_dir: &Path) -> Vec<InstructionFile> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut seen_urls = HashSet::new();

    for pattern in patterns {
        // Handle remote URLs.
        if pattern.starts_with("https://") || pattern.starts_with("http://") {
            if !seen_urls.insert(pattern.clone()) {
                continue;
            }
            if let Some(file) = fetch_remote_instruction(pattern) {
                files.push(file);
            }
            continue;
        }

        let expanded = if let Some(rest) = pattern.strip_prefix("~/") {
            if let Some(home) = dirs_next::home_dir() {
                home.join(rest).to_string_lossy().to_string()
            } else {
                continue;
            }
        } else if !Path::new(pattern).is_absolute() {
            working_dir.join(pattern).to_string_lossy().to_string()
        } else {
            pattern.clone()
        };

        // Use glob to expand patterns
        let matches = match glob::glob(&expanded) {
            Ok(paths) => paths,
            Err(_) => continue,
        };

        for entry in matches {
            let path = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };

            if !path.is_file() {
                continue;
            }

            let canonical = match path.canonicalize() {
                Ok(c) => c,
                Err(_) => continue,
            };

            if !seen.insert(canonical.clone()) {
                continue;
            }

            let content = match std::fs::read_to_string(&canonical) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if content.trim().is_empty() {
                continue;
            }

            let content = truncate_content(content);

            // Strip HTML comments and process includes
            let cleaned = strip_html_comments(&content);
            let include_dir = canonical.parent().unwrap_or(working_dir);
            let (processed, mut included_files) = process_includes(
                &cleaned,
                include_dir,
                working_dir,
                0,
                &mut seen,
                Some(&canonical),
            );

            files.append(&mut included_files);
            files.push(InstructionFile {
                scope: "config".to_string(),
                path: canonical,
                content: processed,
                source: InstructionSource::Config,
                path_globs: None,
                included_from: None,
            });
        }
    }

    files
}

/// Fetch a remote instruction file via HTTP(S) using `curl`.
///
/// Returns `None` on any failure (network error, timeout, non-200 status, empty body).
/// Uses a 5-second timeout to avoid blocking startup.
pub(super) fn fetch_remote_instruction(url: &str) -> Option<InstructionFile> {
    let output = Command::new("curl")
        .args([
            "-sSfL",
            "--max-time",
            &REMOTE_INSTRUCTION_TIMEOUT_SECS.to_string(),
            url,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        tracing::debug!(url = %url, "Failed to fetch remote instruction");
        return None;
    }

    let content = String::from_utf8_lossy(&output.stdout).to_string();
    if content.trim().is_empty() {
        return None;
    }

    let content = truncate_content(content);

    Some(InstructionFile {
        scope: "remote".to_string(),
        path: PathBuf::from(url),
        content,
        source: InstructionSource::Remote,
        path_globs: None,
        included_from: None,
    })
}
