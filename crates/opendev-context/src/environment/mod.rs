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
mod tests;
