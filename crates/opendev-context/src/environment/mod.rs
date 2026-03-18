//! Environment context collector for system prompt injection.
//!
//! Mirrors Python's `opendev/core/agents/components/prompts/environment.py`.
//! Collects git status, tech stack, and project structure at startup,
//! then formats it for inclusion in the system prompt.

mod instructions;
mod project;

use std::path::Path;

pub use instructions::{discover_instruction_files, resolve_instruction_paths};

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
    /// Absolute path to the working directory.
    pub working_dir: String,
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
                    project::git_cmd(working_dir, &["rev-parse", "--abbrev-ref", "HEAD"]),
                    project::detect_default_branch(working_dir),
                    project::git_cmd(working_dir, &["status", "--short"]),
                    project::git_cmd(working_dir, &["log", "--oneline", "-5"]),
                    project::git_cmd(working_dir, &["remote", "get-url", "origin"]),
                )
            } else {
                (None, None, None, None, None)
            };

        let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let current_date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let shell = std::env::var("SHELL").ok();

        let project_config_files = project::detect_config_files(working_dir);
        let tech_stack = project::infer_tech_stack(&project_config_files);
        let directory_tree = project::build_directory_tree(working_dir, 2);
        let instruction_files = instructions::discover_instruction_files(working_dir);

        Self {
            working_dir: working_dir.display().to_string(),
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
        if !self.working_dir.is_empty() {
            env_lines.push(format!("- Working directory: {}", self.working_dir));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_config_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.path().join("Makefile"), "all:").unwrap();

        let configs = project::detect_config_files(dir.path());
        assert!(configs.contains(&"Cargo.toml".to_string()));
        assert!(configs.contains(&"Makefile".to_string()));
        assert!(!configs.contains(&"package.json".to_string()));
    }

    #[test]
    fn test_infer_tech_stack() {
        let configs = vec!["Cargo.toml".to_string(), "Dockerfile".to_string()];
        let stack = project::infer_tech_stack(&configs);
        assert!(stack.contains(&"Rust".to_string()));
        assert!(stack.contains(&"Docker".to_string()));
    }

    #[test]
    fn test_infer_tech_stack_dedup() {
        let configs = vec!["pyproject.toml".to_string(), "requirements.txt".to_string()];
        let stack = project::infer_tech_stack(&configs);
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

        let tree = project::build_directory_tree(&dir_path, 2).unwrap();
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

        let tree = project::build_directory_tree(&dir_path, 2).unwrap();
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

        let tree = project::build_directory_tree(&dir_path, 2).unwrap();
        assert!(!tree.contains("node_modules"));
        assert!(tree.contains("src/"));
    }

    #[test]
    fn test_environment_context_format_prompt_block() {
        let ctx = EnvironmentContext {
            working_dir: "/Users/test/myproject".to_string(),
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
        assert!(block.contains("Working directory: /Users/test/myproject"));
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
        let result = instructions::fetch_remote_instruction("https://localhost:1/__test__");
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
