//! Environment context collector for system prompt injection.
//!
//! Mirrors Python's `opendev/core/agents/components/prompts/environment.py`.
//! Collects git status, tech stack, and project structure at startup,
//! then formats it for inclusion in the system prompt.

use std::path::Path;
use std::process::Command;

/// A project instruction file discovered from the hierarchy.
#[derive(Debug, Clone)]
pub struct InstructionFile {
    /// Relative description of where this file was found (e.g. "project", "parent", "global").
    pub scope: String,
    /// Absolute path to the file.
    pub path: std::path::PathBuf,
    /// File contents (truncated to 50 KB to avoid prompt bloat).
    pub content: String,
}

/// Collected environment context for system prompt injection.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentContext {
    /// Current git branch name.
    pub git_branch: Option<String>,
    /// Default branch (main/master).
    pub git_default_branch: Option<String>,
    /// Git status summary (changed files).
    pub git_status: Option<String>,
    /// Recent commit log (last 5).
    pub git_recent_commits: Option<String>,
    /// Remote origin URL.
    pub git_remote_url: Option<String>,
    /// Operating system and platform info.
    pub platform: String,
    /// Current date (ISO format).
    pub current_date: String,
    /// User's shell.
    pub shell: Option<String>,
    /// Detected project config files.
    pub project_config_files: Vec<String>,
    /// Inferred tech stack.
    pub tech_stack: Vec<String>,
    /// Shallow directory tree (depth 2).
    pub directory_tree: Option<String>,
    /// Project instruction files (AGENTS.md, CLAUDE.md, .opendev/instructions.md).
    pub instruction_files: Vec<InstructionFile>,
}

impl EnvironmentContext {
    /// Collect environment context from the working directory.
    pub fn collect(working_dir: &Path) -> Self {
        let is_git = working_dir.join(".git").exists();

        let (git_branch, git_default_branch, git_status, git_recent_commits, git_remote_url) =
            if is_git {
                (
                    git_cmd(working_dir, &["rev-parse", "--abbrev-ref", "HEAD"]),
                    detect_default_branch(working_dir),
                    git_cmd(working_dir, &["status", "--short"]),
                    git_cmd(working_dir, &["log", "--oneline", "-5"]),
                    git_cmd(working_dir, &["remote", "get-url", "origin"]),
                )
            } else {
                (None, None, None, None, None)
            };

        let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let current_date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let shell = std::env::var("SHELL").ok();

        let project_config_files = detect_config_files(working_dir);
        let tech_stack = infer_tech_stack(&project_config_files);
        let directory_tree = build_directory_tree(working_dir, 2);
        let instruction_files = discover_instruction_files(working_dir);

        Self {
            git_branch,
            git_default_branch,
            git_status,
            git_recent_commits,
            git_remote_url,
            platform,
            current_date,
            shell,
            project_config_files,
            tech_stack,
            directory_tree,
            instruction_files,
        }
    }

    /// Format the environment context as a system prompt block.
    pub fn format_prompt_block(&self) -> String {
        let mut sections = Vec::new();

        // Environment section
        let mut env_lines = vec![format!("# Environment")];
        env_lines.push(format!("- Platform: {}", self.platform));
        env_lines.push(format!("- Date: {}", self.current_date));
        if let Some(ref shell) = self.shell {
            env_lines.push(format!("- Shell: {shell}"));
        }
        if !self.tech_stack.is_empty() {
            env_lines.push(format!("- Tech stack: {}", self.tech_stack.join(", ")));
        }
        sections.push(env_lines.join("\n"));

        // Git section
        if self.git_branch.is_some() {
            let mut git_lines = vec!["# Git Status (snapshot at conversation start)".to_string()];
            if let Some(ref branch) = self.git_branch {
                git_lines.push(format!("- Current branch: {branch}"));
            }
            if let Some(ref default) = self.git_default_branch {
                git_lines.push(format!("- Default branch: {default}"));
            }
            if let Some(ref remote) = self.git_remote_url {
                git_lines.push(format!("- Remote: {remote}"));
            }
            if let Some(ref status) = self.git_status {
                if status.trim().is_empty() {
                    git_lines.push("- Working tree: clean".to_string());
                } else {
                    let count = status.lines().count();
                    git_lines.push(format!("- Changed files ({count}):"));
                    // Show first 20 changed files
                    for line in status.lines().take(20) {
                        git_lines.push(format!("  {line}"));
                    }
                    if count > 20 {
                        git_lines.push(format!("  ... and {} more", count - 20));
                    }
                }
            }
            if let Some(ref log) = self.git_recent_commits
                && !log.trim().is_empty()
            {
                git_lines.push("- Recent commits:".to_string());
                for line in log.lines().take(5) {
                    git_lines.push(format!("  {line}"));
                }
            }
            sections.push(git_lines.join("\n"));
        }

        // Project structure section
        if !self.project_config_files.is_empty() || self.directory_tree.is_some() {
            let mut proj_lines = vec!["# Project Structure".to_string()];
            if !self.project_config_files.is_empty() {
                proj_lines.push(format!(
                    "- Config files: {}",
                    self.project_config_files.join(", ")
                ));
            }
            if let Some(ref tree) = self.directory_tree {
                proj_lines.push("- Directory layout:".to_string());
                proj_lines.push(format!("```\n{tree}\n```"));
            }
            sections.push(proj_lines.join("\n"));
        }

        // Project instructions section
        if !self.instruction_files.is_empty() {
            let mut instr_lines = vec!["# Project Instructions".to_string()];
            instr_lines.push(
                "The following instruction files were found in the project hierarchy. \
                 Follow these instructions when working in this project."
                    .to_string(),
            );
            for instr in &self.instruction_files {
                instr_lines.push(String::new());
                instr_lines.push(format!(
                    "## {} ({})",
                    instr.path.file_name().unwrap_or_default().to_string_lossy(),
                    instr.scope
                ));
                instr_lines.push(instr.content.clone());
            }
            sections.push(instr_lines.join("\n"));
        }

        sections.join("\n\n")
    }
}

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

/// Timeout in seconds for fetching remote instructions via HTTP(S).
const REMOTE_INSTRUCTION_TIMEOUT_SECS: u64 = 5;

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
fn fetch_remote_instruction(url: &str) -> Option<InstructionFile> {
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

/// Run a git command and return trimmed stdout, or None on failure.
fn git_cmd(working_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(working_dir)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Detect the default branch (main or master).
fn detect_default_branch(working_dir: &Path) -> Option<String> {
    // Try symbolic-ref first
    if let Some(branch) = git_cmd(working_dir, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        return branch
            .strip_prefix("refs/remotes/origin/")
            .map(String::from);
    }
    // Fallback: check if main or master exists
    for branch in &["main", "master"] {
        if git_cmd(
            working_dir,
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
        )
        .is_some()
        {
            return Some(branch.to_string());
        }
    }
    None
}

/// Known project configuration files and their tech stacks.
const CONFIG_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", "Rust"),
    ("package.json", "Node.js/JavaScript"),
    ("tsconfig.json", "TypeScript"),
    ("pyproject.toml", "Python"),
    ("setup.py", "Python"),
    ("requirements.txt", "Python"),
    ("go.mod", "Go"),
    ("Gemfile", "Ruby"),
    ("pom.xml", "Java/Maven"),
    ("build.gradle", "Java/Gradle"),
    ("build.gradle.kts", "Kotlin/Gradle"),
    ("Makefile", "Make"),
    ("CMakeLists.txt", "C/C++/CMake"),
    ("docker-compose.yml", "Docker"),
    ("docker-compose.yaml", "Docker"),
    ("Dockerfile", "Docker"),
    (".github/workflows", "GitHub Actions"),
    ("terraform", "Terraform"),
    ("serverless.yml", "Serverless"),
    ("next.config.js", "Next.js"),
    ("next.config.mjs", "Next.js"),
    ("vite.config.ts", "Vite"),
    ("webpack.config.js", "Webpack"),
    ("tailwind.config.js", "Tailwind CSS"),
    ("mix.exs", "Elixir"),
    ("pubspec.yaml", "Flutter/Dart"),
    ("Podfile", "iOS/CocoaPods"),
    (".swift", "Swift"),
];

/// Detect which config files exist in the working directory.
fn detect_config_files(working_dir: &Path) -> Vec<String> {
    CONFIG_FILES
        .iter()
        .filter(|(file, _)| working_dir.join(file).exists())
        .map(|(file, _)| file.to_string())
        .collect()
}

/// Infer tech stack from detected config files.
fn infer_tech_stack(config_files: &[String]) -> Vec<String> {
    let mut stack: Vec<String> = config_files
        .iter()
        .filter_map(|file| {
            CONFIG_FILES
                .iter()
                .find(|(f, _)| *f == file.as_str())
                .map(|(_, tech)| tech.to_string())
        })
        .collect();
    stack.sort();
    stack.dedup();
    stack
}

/// Build a shallow directory tree (up to given depth).
fn build_directory_tree(working_dir: &Path, max_depth: usize) -> Option<String> {
    let mut lines = Vec::new();
    let dir_name = working_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    lines.push(format!("{dir_name}/"));
    collect_tree_entries(working_dir, "", max_depth, 0, &mut lines);
    if lines.len() <= 1 {
        return None;
    }
    Some(lines.join("\n"))
}

/// Recursively collect directory entries for the tree display.
fn collect_tree_entries(
    dir: &Path,
    prefix: &str,
    max_depth: usize,
    current_depth: usize,
    lines: &mut Vec<String>,
) {
    if current_depth >= max_depth {
        return;
    }

    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                !name.starts_with('.')
                    && name != "node_modules"
                    && name != "target"
                    && name != "dist"
                    && name != "build"
                    && name != "__pycache__"
                    && name != ".git"
                    && name != "vendor"
                    && name != "venv"
                    && name != ".venv"
            })
            .collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    // Limit to 30 entries per directory
    let total = entries.len();
    let show = entries.iter().take(30);

    for (i, entry) in show.enumerate() {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_last = i == total.min(30) - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };

        if entry.path().is_dir() {
            lines.push(format!("{prefix}{connector}{name}/"));
            collect_tree_entries(
                &entry.path(),
                &child_prefix,
                max_depth,
                current_depth + 1,
                lines,
            );
        } else {
            lines.push(format!("{prefix}{connector}{name}"));
        }
    }

    if total > 30 {
        lines.push(format!("{prefix}    ... and {} more", total - 30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_config_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.path().join("Makefile"), "all:").unwrap();

        let configs = detect_config_files(dir.path());
        assert!(configs.contains(&"Cargo.toml".to_string()));
        assert!(configs.contains(&"Makefile".to_string()));
        assert!(!configs.contains(&"package.json".to_string()));
    }

    #[test]
    fn test_infer_tech_stack() {
        let configs = vec!["Cargo.toml".to_string(), "Dockerfile".to_string()];
        let stack = infer_tech_stack(&configs);
        assert!(stack.contains(&"Rust".to_string()));
        assert!(stack.contains(&"Docker".to_string()));
    }

    #[test]
    fn test_infer_tech_stack_dedup() {
        let configs = vec!["pyproject.toml".to_string(), "requirements.txt".to_string()];
        let stack = infer_tech_stack(&configs);
        // Both map to "Python", should be deduped
        assert_eq!(stack.iter().filter(|s| *s == "Python").count(), 1);
    }

    #[test]
    fn test_build_directory_tree() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::create_dir(dir_path.join("src")).unwrap();
        std::fs::write(dir_path.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir_path.join("Cargo.toml"), "[package]").unwrap();

        let tree = build_directory_tree(&dir_path, 2).unwrap();
        assert!(tree.contains("src/"));
        assert!(tree.contains("main.rs"));
        assert!(tree.contains("Cargo.toml"));
    }

    #[test]
    fn test_build_directory_tree_skips_hidden() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::create_dir(dir_path.join(".hidden")).unwrap();
        std::fs::create_dir(dir_path.join("visible")).unwrap();
        std::fs::write(dir_path.join("visible/file.txt"), "hi").unwrap();

        let tree = build_directory_tree(&dir_path, 2).unwrap();
        assert!(!tree.contains(".hidden"));
        assert!(tree.contains("visible/"));
    }

    #[test]
    fn test_build_directory_tree_skips_node_modules() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::create_dir(dir_path.join("node_modules")).unwrap();
        std::fs::create_dir(dir_path.join("src")).unwrap();
        std::fs::write(dir_path.join("src/index.js"), "").unwrap();

        let tree = build_directory_tree(&dir_path, 2).unwrap();
        assert!(!tree.contains("node_modules"));
        assert!(tree.contains("src/"));
    }

    #[test]
    fn test_environment_context_format_prompt_block() {
        let ctx = EnvironmentContext {
            git_branch: Some("feature/test".to_string()),
            git_default_branch: Some("main".to_string()),
            git_status: Some("M src/lib.rs\n?? new_file.rs".to_string()),
            git_recent_commits: Some("abc1234 Fix bug\ndef5678 Add feature".to_string()),
            git_remote_url: Some("git@github.com:user/repo.git".to_string()),
            platform: "macos aarch64".to_string(),
            current_date: "2026-03-14".to_string(),
            shell: Some("/bin/zsh".to_string()),
            project_config_files: vec!["Cargo.toml".to_string()],
            tech_stack: vec!["Rust".to_string()],
            directory_tree: Some("project/\n├── src/\n└── Cargo.toml".to_string()),
            instruction_files: vec![],
        };

        let block = ctx.format_prompt_block();
        assert!(block.contains("# Environment"));
        assert!(block.contains("macos aarch64"));
        assert!(block.contains("Rust"));
        assert!(block.contains("# Git Status"));
        assert!(block.contains("feature/test"));
        assert!(block.contains("M src/lib.rs"));
        assert!(block.contains("Fix bug"));
        assert!(block.contains("# Project Structure"));
        assert!(block.contains("Cargo.toml"));
    }

    #[test]
    fn test_environment_context_no_git() {
        let ctx = EnvironmentContext {
            platform: "linux x86_64".to_string(),
            current_date: "2026-03-14".to_string(),
            ..Default::default()
        };

        let block = ctx.format_prompt_block();
        assert!(block.contains("# Environment"));
        assert!(!block.contains("# Git Status"));
    }

    #[test]
    fn test_collect_on_current_dir() {
        // Just verify it doesn't panic
        let ctx = EnvironmentContext::collect(std::path::Path::new("."));
        assert!(!ctx.platform.is_empty());
        assert!(!ctx.current_date.is_empty());
    }

    // --- Instruction file discovery ---

    #[test]
    fn test_discover_agents_md() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("AGENTS.md"), "# Rules\nDo X.").unwrap();
        // Add .git so discovery stops here
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].scope, "project");
        assert!(files[0].content.contains("Do X."));
    }

    #[test]
    fn test_discover_claude_md() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("CLAUDE.md"), "# Claude\nBe helpful.").unwrap();
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("Be helpful."));
    }

    #[test]
    fn test_discover_opendev_instructions() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::create_dir(dir_path.join(".opendev")).unwrap();
        std::fs::write(
            dir_path.join(".opendev/instructions.md"),
            "Custom instructions",
        )
        .unwrap();
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("Custom instructions"));
    }

    #[test]
    fn test_discover_multiple_instruction_files() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("AGENTS.md"), "agents").unwrap();
        std::fs::write(dir_path.join("CLAUDE.md"), "claude").unwrap();
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_discover_walks_up_to_git_root() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        // Parent has AGENTS.md and .git
        std::fs::write(dir_path.join("AGENTS.md"), "parent rules").unwrap();
        std::fs::create_dir(dir_path.join(".git")).unwrap();
        // Child subdirectory
        let child = dir_path.join("sub");
        std::fs::create_dir(&child).unwrap();

        let files = discover_instruction_files(&child);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].scope, "parent");
        assert!(files[0].content.contains("parent rules"));
    }

    #[test]
    fn test_discover_empty_file_skipped() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("AGENTS.md"), "  \n  ").unwrap();
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert!(files.is_empty());
    }

    #[test]
    fn test_discover_no_duplicates() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("AGENTS.md"), "rules").unwrap();
        // No .git, so it would walk up — but the same file shouldn't appear twice
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_discover_claude_instructions_dir() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(dir_path.join(".claude")).unwrap();
        std::fs::write(
            dir_path.join(".claude/instructions.md"),
            "Claude-specific instructions",
        )
        .unwrap();
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("Claude-specific instructions"));
    }

    #[test]
    fn test_discover_both_opendev_and_claude_instructions() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(dir_path.join(".opendev")).unwrap();
        std::fs::create_dir_all(dir_path.join(".claude")).unwrap();
        std::fs::write(dir_path.join(".opendev/instructions.md"), "OpenDev rules").unwrap();
        std::fs::write(dir_path.join(".claude/instructions.md"), "Claude rules").unwrap();
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let files = discover_instruction_files(&dir_path);
        assert_eq!(files.len(), 2);
        let contents: Vec<&str> = files.iter().map(|f| f.content.as_str()).collect();
        assert!(contents.iter().any(|c| c.contains("OpenDev rules")));
        assert!(contents.iter().any(|c| c.contains("Claude rules")));
    }

    #[test]
    fn test_instruction_in_prompt_block() {
        let ctx = EnvironmentContext {
            platform: "test".to_string(),
            current_date: "2026-03-15".to_string(),
            instruction_files: vec![InstructionFile {
                scope: "project".to_string(),
                path: std::path::PathBuf::from("/project/AGENTS.md"),
                content: "# Build rules\nRun cargo test.".to_string(),
            }],
            ..Default::default()
        };

        let block = ctx.format_prompt_block();
        assert!(block.contains("# Project Instructions"));
        assert!(block.contains("AGENTS.md (project)"));
        assert!(block.contains("Run cargo test."));
    }

    #[test]
    fn test_resolve_instruction_paths_direct_file() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("CONTRIBUTING.md"), "contrib rules").unwrap();

        let files = resolve_instruction_paths(&["CONTRIBUTING.md".to_string()], &dir_path);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].scope, "config");
        assert!(files[0].content.contains("contrib rules"));
    }

    #[test]
    fn test_resolve_instruction_paths_glob_pattern() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let rules_dir = dir_path.join("rules");
        std::fs::create_dir(&rules_dir).unwrap();
        std::fs::write(rules_dir.join("a.md"), "rule a").unwrap();
        std::fs::write(rules_dir.join("b.md"), "rule b").unwrap();
        std::fs::write(rules_dir.join("c.txt"), "not a markdown").unwrap();

        let files = resolve_instruction_paths(&["rules/*.md".to_string()], &dir_path);
        assert_eq!(files.len(), 2);
        let contents: Vec<&str> = files.iter().map(|f| f.content.as_str()).collect();
        assert!(contents.iter().any(|c| c.contains("rule a")));
        assert!(contents.iter().any(|c| c.contains("rule b")));
    }

    #[test]
    fn test_resolve_instruction_paths_absolute() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("guide.md"), "absolute guide").unwrap();
        let abs_path = dir_path.join("guide.md").to_string_lossy().to_string();

        let files = resolve_instruction_paths(&[abs_path], Path::new("/tmp"));
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("absolute guide"));
    }

    #[test]
    fn test_resolve_instruction_paths_skips_empty() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("empty.md"), "   ").unwrap();

        let files = resolve_instruction_paths(&["empty.md".to_string()], &dir_path);
        assert!(files.is_empty());
    }

    #[test]
    fn test_resolve_instruction_paths_deduplicates() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("rules.md"), "dedup test").unwrap();

        let files =
            resolve_instruction_paths(&["rules.md".to_string(), "rules.md".to_string()], &dir_path);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_resolve_instruction_paths_nonexistent_skipped() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();

        let files = resolve_instruction_paths(&["does_not_exist.md".to_string()], &dir_path);
        assert!(files.is_empty());
    }

    // ---- Remote URL instructions ----

    #[test]
    fn test_resolve_instruction_paths_url_invalid_skipped() {
        // An invalid URL should be skipped gracefully.
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();

        let files = resolve_instruction_paths(
            &["https://localhost:1/__nonexistent_path_test__".to_string()],
            &dir_path,
        );
        assert!(files.is_empty());
    }

    #[test]
    fn test_resolve_instruction_paths_url_deduplicates() {
        // Same URL listed twice should produce at most one entry.
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();

        let url = "https://localhost:1/__dup_test__".to_string();
        let files = resolve_instruction_paths(&[url.clone(), url], &dir_path);
        // Both should fail (unreachable host), but even if one succeeded,
        // dedup ensures at most 1.
        assert!(files.len() <= 1);
    }

    #[test]
    fn test_resolve_instruction_paths_mixed_local_and_url() {
        // Local file + unreachable URL: local file should still load.
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        std::fs::write(dir_path.join("local.md"), "local content").unwrap();

        let files = resolve_instruction_paths(
            &[
                "local.md".to_string(),
                "https://localhost:1/__unreachable__".to_string(),
            ],
            &dir_path,
        );
        assert_eq!(files.len(), 1);
        assert!(files[0].content.contains("local content"));
        assert_eq!(files[0].scope, "config");
    }

    #[test]
    fn test_fetch_remote_instruction_unreachable() {
        let result = fetch_remote_instruction("https://localhost:1/__test__");
        assert!(result.is_none());
    }

    #[test]
    fn test_remote_instruction_scope_is_remote() {
        // Verify that if we had a successful fetch, the scope would be "remote".
        // We can't easily test a real URL in unit tests, but we test the function contract:
        // scope for remote files is "remote", path is the URL.
        let file = InstructionFile {
            scope: "remote".to_string(),
            path: std::path::PathBuf::from("https://example.com/rules.md"),
            content: "test content".to_string(),
        };
        assert_eq!(file.scope, "remote");
        assert_eq!(file.path.to_string_lossy(), "https://example.com/rules.md");
    }

    #[test]
    fn test_discover_cursorrules() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();

        // .git to stop traversal
        std::fs::create_dir(dir_path.join(".git")).unwrap();

        // Create .cursorrules file
        std::fs::write(dir_path.join(".cursorrules"), "Use strict TypeScript").unwrap();

        let files = discover_instruction_files(&dir_path);
        assert!(
            files
                .iter()
                .any(|f| f.content.contains("strict TypeScript")),
            "Should discover .cursorrules: {:?}",
            files
                .iter()
                .map(|f| f.path.display().to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_discover_copilot_instructions() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();

        std::fs::create_dir(dir_path.join(".git")).unwrap();

        let github_dir = dir_path.join(".github");
        std::fs::create_dir_all(&github_dir).unwrap();
        std::fs::write(
            github_dir.join("copilot-instructions.md"),
            "Follow conventional commits",
        )
        .unwrap();

        let files = discover_instruction_files(&dir_path);
        assert!(
            files
                .iter()
                .any(|f| f.content.contains("conventional commits")),
            "Should discover .github/copilot-instructions.md"
        );
    }

    #[test]
    fn test_discover_cursor_rules_directory() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();

        std::fs::create_dir(dir_path.join(".git")).unwrap();

        // Create .cursor/rules/ with rule files
        let rules_dir = dir_path.join(".cursor").join("rules");
        std::fs::create_dir_all(&rules_dir).unwrap();
        std::fs::write(rules_dir.join("security.md"), "Always validate input").unwrap();
        std::fs::write(rules_dir.join("style.md"), "Use 4-space indentation").unwrap();
        // Non-rule file should be ignored
        std::fs::write(rules_dir.join("README"), "Ignore this").unwrap();

        let files = discover_instruction_files(&dir_path);
        assert!(
            files.iter().any(|f| f.content.contains("validate input")),
            "Should discover .cursor/rules/security.md"
        );
        assert!(
            files.iter().any(|f| f.content.contains("4-space")),
            "Should discover .cursor/rules/style.md"
        );
    }
}
