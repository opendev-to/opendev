//! Environment context collector for system prompt injection.
//!
//! Mirrors Python's `opendev/core/agents/components/prompts/environment.py`.
//! Collects git status, tech stack, and project structure at startup,
//! then formats it for inclusion in the system prompt.

mod conditional;
mod exclusions;
pub(crate) mod frontmatter;
mod include_parser;
mod instructions;
mod project;

use std::path::{Path, PathBuf};

pub use conditional::rule_applies;
pub use exclusions::is_excluded;
pub use frontmatter::{Frontmatter, parse_frontmatter, strip_html_comments};
pub use include_parser::process_includes;
pub use instructions::{discover_instruction_files, resolve_instruction_paths};

/// How an instruction file was loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstructionSource {
    /// Found via directory walk (AGENTS.md, CLAUDE.md, etc.).
    Discovery,
    /// From settings.json `instructions` array.
    Config,
    /// Fetched via HTTP(S) URL.
    Remote,
    /// Loaded via @include directive inside another instruction file.
    Include,
    /// Enterprise managed instruction (/etc/opendev/).
    Managed,
    /// Local override (AGENTS.local.md / CLAUDE.local.md), gitignored.
    Local,
    /// From .opendev/rules/*.md directory.
    Rules,
}

/// A project instruction file discovered from the hierarchy.
#[derive(Debug, Clone)]
pub struct InstructionFile {
    /// Relative description of where this file was found (e.g. "project", "parent", "global").
    pub scope: String,
    /// Absolute path to the file.
    pub path: std::path::PathBuf,
    /// File contents (truncated to 50 KB to avoid prompt bloat).
    pub content: String,
    /// How this file was loaded.
    pub source: InstructionSource,
    /// For conditional rules: glob patterns from frontmatter `paths` field.
    /// `None` means the rule always applies.
    pub path_globs: Option<Vec<String>>,
    /// The parent file that @included this file, if any.
    pub included_from: Option<PathBuf>,
}

impl InstructionFile {
    /// Create a new instruction file with default source fields.
    pub fn new(scope: impl Into<String>, path: PathBuf, content: String) -> Self {
        Self {
            scope: scope.into(),
            path,
            content,
            source: InstructionSource::Discovery,
            path_globs: None,
            included_from: None,
        }
    }
}

/// Maximum memory content size to inject (25 KB).
const MAX_MEMORY_BYTES: usize = 25 * 1024;
/// Maximum lines for MEMORY.md injection.
const MAX_MEMORY_LINES: usize = 200;

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
    /// Model name (set by CLI after collection).
    pub model_name: Option<String>,
    /// Project memory content (MEMORY.md), loaded at startup.
    pub memory_content: Option<String>,
}

impl EnvironmentContext {
    /// Collect environment context from the working directory.
    ///
    /// Git commands and directory scanning run in parallel to minimize startup latency.
    /// `exclude_patterns` filters out instruction files matching any glob pattern.
    /// `additional_dirs` adds extra directories for instruction discovery.
    pub fn collect(working_dir: &Path) -> Self {
        Self::collect_with_options(working_dir, &[], &[])
    }

    /// Collect with instruction filtering options.
    pub fn collect_with_options(
        working_dir: &Path,
        exclude_patterns: &[String],
        additional_dirs: &[PathBuf],
    ) -> Self {
        let is_git = working_dir.join(".git").exists();

        // Run all git commands and directory scanning in parallel using scoped threads.
        // Each git command spawns a subprocess, so parallelizing them avoids
        // sequential process-spawn overhead (significant on large repos).
        let (
            git_branch,
            git_default_branch,
            git_status,
            git_recent_commits,
            git_remote_url,
            directory_tree,
            instruction_files,
            memory_content,
        ) = if is_git {
            std::thread::scope(|s| {
                let h_branch = s.spawn(|| {
                    project::git_cmd(working_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
                });
                let h_default = s.spawn(|| project::detect_default_branch(working_dir));
                let h_status = s.spawn(|| project::git_cmd(working_dir, &["status", "--short"]));
                let h_log = s.spawn(|| project::git_cmd(working_dir, &["log", "--oneline", "-5"]));
                let h_remote =
                    s.spawn(|| project::git_cmd(working_dir, &["remote", "get-url", "origin"]));
                let h_tree = s.spawn(|| project::build_directory_tree(working_dir, 2));
                let h_instr = s.spawn(|| {
                    instructions::discover_instruction_files(
                        working_dir,
                        exclude_patterns,
                        additional_dirs,
                    )
                });
                let h_memory = s.spawn(|| load_project_memory(working_dir));

                (
                    h_branch.join().unwrap_or(None),
                    h_default.join().unwrap_or(None),
                    h_status.join().unwrap_or(None),
                    h_log.join().unwrap_or(None),
                    h_remote.join().unwrap_or(None),
                    h_tree.join().unwrap_or(None),
                    h_instr.join().unwrap_or_default(),
                    h_memory.join().unwrap_or(None),
                )
            })
        } else {
            let directory_tree = project::build_directory_tree(working_dir, 2);
            let instruction_files = instructions::discover_instruction_files(
                working_dir,
                exclude_patterns,
                additional_dirs,
            );
            let memory_content = load_project_memory(working_dir);
            (
                None,
                None,
                None,
                None,
                None,
                directory_tree,
                instruction_files,
                memory_content,
            )
        };

        let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let current_date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let shell = std::env::var("SHELL").ok();

        let project_config_files = project::detect_config_files(working_dir);
        let tech_stack = project::infer_tech_stack(&project_config_files);

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
            model_name: None,
            memory_content,
        }
    }

    /// Format the environment context as a system prompt block.
    pub fn format_prompt_block(&self) -> String {
        self.format_prompt_block_with_context(&[])
    }

    /// Format the environment context, filtering conditional rules by active files.
    ///
    /// Instructions with `path_globs` are only included if at least one active file
    /// matches their glob patterns. Instructions without `path_globs` are always included.
    pub fn format_prompt_block_with_context(&self, active_files: &[PathBuf]) -> String {
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
        if let Some(ref model) = self.model_name {
            env_lines.push(format!("- Model: {model}"));
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
        // Filter conditional rules by active files.
        let working_dir = PathBuf::from(&self.working_dir);
        let applicable: Vec<_> = self
            .instruction_files
            .iter()
            .filter(|instr| {
                conditional::rule_applies(instr.path_globs.as_deref(), active_files, &working_dir)
            })
            .collect();
        if !applicable.is_empty() {
            let mut instr_lines = vec!["# Project Instructions".to_string()];
            instr_lines.push(
                "The following instruction files were found in the project hierarchy. \
                 Follow these instructions when working in this project."
                    .to_string(),
            );
            for instr in &applicable {
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

        // Project memory section
        if let Some(ref memory) = self.memory_content {
            sections.push(format!(
                "# Project Memory\n\
                 Your persistent memory for this project (from MEMORY.md). \
                 Use the `memory` tool to update.\n\n{memory}"
            ));
        }

        sections.join("\n\n")
    }
}

/// Load project-scoped MEMORY.md for system prompt injection.
fn load_project_memory(working_dir: &Path) -> Option<String> {
    let paths = opendev_config::paths::Paths::new(Some(working_dir.to_path_buf()));
    let content = std::fs::read_to_string(paths.project_memory_index()).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let truncated: String = content
        .lines()
        .take(MAX_MEMORY_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if truncated.len() > MAX_MEMORY_BYTES {
        Some(truncated[..MAX_MEMORY_BYTES].to_string())
    } else {
        Some(truncated)
    }
}

#[cfg(test)]
mod tests;
