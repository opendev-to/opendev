//! Instruction file discovery and resolution.
//!
//! Discovers project instruction files (AGENTS.md, CLAUDE.md, etc.) by walking
//! the directory hierarchy, and resolves config-specified instruction paths
//! (globs, URLs, ~/paths).

use std::path::Path;
use std::process::Command;

use super::InstructionFile;

/// Instruction file names to search for, in priority order.
const INSTRUCTION_FILENAMES: &[&str] = &["AGENTS.md", "CLAUDE.md", "OPENDEV.md"];

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
/// Searches for `AGENTS.md`, `CLAUDE.md`, `OPENDEV.md` in the working directory
/// and each parent up to the filesystem root (or git root). Also checks
/// `.opendev/instructions.md` and `.claude/instructions.md` in each directory,
/// and global config at `~/.opendev/instructions.md`, `~/.config/opendev/AGENTS.md`,
/// and `~/.claude/CLAUDE.md`.
///
/// Files found closer to `working_dir` have higher priority and are listed first.
pub fn discover_instruction_files(working_dir: &Path) -> Vec<InstructionFile> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Walk up the directory tree from working_dir
    let mut current = working_dir.to_path_buf();
    loop {
        // Check each instruction filename
        for filename in INSTRUCTION_FILENAMES {
            let candidate = current.join(filename);
            try_add_instruction(&candidate, &current, working_dir, &mut files, &mut seen);
        }

        // Check .opendev/instructions.md and .claude/instructions.md
        let opendev_instr = current.join(".opendev").join("instructions.md");
        try_add_instruction(&opendev_instr, &current, working_dir, &mut files, &mut seen);
        let claude_instr = current.join(".claude").join("instructions.md");
        try_add_instruction(&claude_instr, &current, working_dir, &mut files, &mut seen);

        // Check compatibility files from other AI tools (.cursorrules, copilot, etc.)
        for compat_path in COMPAT_INSTRUCTION_FILES {
            let candidate = current.join(compat_path);
            try_add_instruction(&candidate, &current, working_dir, &mut files, &mut seen);
        }

        // Check .cursor/rules/ directory for individual rule files
        let cursor_rules_dir = current.join(".cursor").join("rules");
        if cursor_rules_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&cursor_rules_dir)
        {
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
                try_add_instruction(&entry.path(), &current, working_dir, &mut files, &mut seen);
            }
        }

        // Stop at git root or filesystem root
        if current.join(".git").exists() {
            break;
        }
        if !current.pop() {
            break;
        }
    }

    // Check global config locations
    if let Some(home) = dirs_next::home_dir() {
        let global_paths = [
            home.join(".opendev").join("instructions.md"),
            home.join(".opendev").join("AGENTS.md"),
            home.join(".config").join("opendev").join("AGENTS.md"),
            home.join(".claude").join("CLAUDE.md"),
        ];
        for path in &global_paths {
            try_add_instruction(
                path,
                path.parent().unwrap_or(path),
                working_dir,
                &mut files,
                &mut seen,
            );
        }
    }

    files
}

/// Try to read an instruction file and add it to the list if it exists.
fn try_add_instruction(
    path: &Path,
    dir: &Path,
    working_dir: &Path,
    files: &mut Vec<InstructionFile>,
    seen: &mut std::collections::HashSet<std::path::PathBuf>,
) {
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => return, // File doesn't exist
    };
    if !seen.insert(canonical.clone()) {
        return; // Already seen (e.g. symlink or parent overlap)
    }

    let content = match std::fs::read_to_string(&canonical) {
        Ok(c) => c,
        Err(_) => return,
    };
    if content.trim().is_empty() {
        return;
    }

    // Truncate if too large
    let content = if content.len() > MAX_INSTRUCTION_BYTES {
        let truncated = &content[..MAX_INSTRUCTION_BYTES];
        format!(
            "{truncated}\n\n... (truncated, file is {} KB)",
            content.len() / 1024
        )
    } else {
        content
    };

    let scope = if dir == working_dir || dir.starts_with(working_dir) {
        "project".to_string()
    } else if dir.to_string_lossy().contains(".opendev")
        || dir.to_string_lossy().contains(".config")
        || dir.to_string_lossy().contains(".claude")
    {
        "global".to_string()
    } else {
        "parent".to_string()
    };

    files.push(InstructionFile {
        scope,
        path: canonical,
        content,
    });
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
    let mut seen = std::collections::HashSet::new();
    let mut seen_urls = std::collections::HashSet::new();

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

            let content = if content.len() > MAX_INSTRUCTION_BYTES {
                let truncated = &content[..MAX_INSTRUCTION_BYTES];
                format!(
                    "{truncated}\n\n... (truncated, file is {} KB)",
                    content.len() / 1024
                )
            } else {
                content
            };

            files.push(InstructionFile {
                scope: "config".to_string(),
                path: canonical,
                content,
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

    let content = if content.len() > MAX_INSTRUCTION_BYTES {
        let truncated = &content[..MAX_INSTRUCTION_BYTES];
        format!(
            "{truncated}\n\n... (truncated, remote file is {} KB)",
            content.len() / 1024
        )
    } else {
        content
    };

    Some(InstructionFile {
        scope: "remote".to_string(),
        path: std::path::PathBuf::from(url),
        content,
    })
}
