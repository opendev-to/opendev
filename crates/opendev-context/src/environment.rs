//! Environment context collector for system prompt injection.
//!
//! Mirrors Python's `opendev/core/agents/components/prompts/environment.py`.
//! Collects git status, tech stack, and project structure at startup,
//! then formats it for inclusion in the system prompt.

use std::path::Path;
use std::process::Command;

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

        sections.join("\n\n")
    }
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
}
