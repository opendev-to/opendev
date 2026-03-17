//! Custom commands loaded from `.opendev/commands/` and `.claude/commands/`.
//!
//! Text files in the commands directories become slash commands. Supports:
//! - YAML frontmatter for metadata (`description`, `model`, `agent`, `subtask`)
//! - `$1`, `$2`, etc. for positional arguments
//! - `$ARGUMENTS` for all arguments
//! - Context variable substitution (`$KEY` → value)
//! - Shell substitution: `!`cmd`` executes the command and inlines its output
//!
//! # Example
//!
//! `.opendev/commands/review.md` contains:
//! ```text
//! ---
//! description: "Code review with security focus"
//! model: gpt-4o
//! subtask: true
//! ---
//!
//! Review this code for: $ARGUMENTS
//! Current branch: !`git branch --show-current`
//! Focus on security and performance.
//! ```

mod expansion;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tracing::debug;

use expansion::parse_frontmatter;

/// A custom command loaded from a text file.
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// Command name (derived from filename).
    pub name: String,
    /// Template text with placeholder variables (frontmatter stripped).
    pub template: String,
    /// Source identifier (e.g. "project:review.md").
    pub source: String,
    /// Human-readable description (from frontmatter or first `#` line).
    pub description: String,
    /// Optional model override for this command.
    pub model: Option<String>,
    /// Optional agent override for this command.
    pub agent: Option<String>,
    /// Whether this command should run as a subtask (restricted permissions).
    pub subtask: bool,
}

/// Summary info for listing commands.
#[derive(Debug, Clone)]
pub struct CommandInfo {
    /// Command name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Source identifier.
    pub source: String,
    /// Optional model override.
    pub model: Option<String>,
    /// Optional agent override.
    pub agent: Option<String>,
}

/// Loads and manages custom commands from command directories.
pub struct CustomCommandLoader {
    working_dir: PathBuf,
    commands: Option<HashMap<String, CustomCommand>>,
}

impl CustomCommandLoader {
    /// Create a new loader rooted at the given working directory.
    pub fn new(working_dir: &Path) -> Self {
        Self {
            working_dir: working_dir.to_path_buf(),
            commands: None,
        }
    }

    /// Load all custom commands from command directories.
    ///
    /// Scans `.opendev/commands/`, `.claude/commands/` under the project
    /// directory (higher priority) and then global directories. Results are cached.
    pub fn load_commands(&mut self) -> &HashMap<String, CustomCommand> {
        if let Some(ref cmds) = self.commands {
            return cmds;
        }

        let mut commands = HashMap::new();
        let dirs = self.get_command_dirs();

        for (cmd_dir, source) in dirs {
            if let Ok(entries) = fs::read_dir(&cmd_dir) {
                let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                paths.sort();

                for path in paths {
                    if !path.is_file() {
                        continue;
                    }

                    // Only .md, .txt, or extensionless files
                    let ext = path.extension().and_then(|e| e.to_str());
                    match ext {
                        Some("md") | Some("txt") | None => {}
                        _ => continue,
                    }

                    let stem = match path.file_stem().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };

                    // Skip hidden/private files
                    if stem.starts_with('.') || stem.starts_with('_') {
                        continue;
                    }

                    match fs::read_to_string(&path) {
                        Ok(raw_content) => {
                            let (frontmatter, template) = parse_frontmatter(&raw_content);

                            // Description: frontmatter > first # line > empty
                            let description = frontmatter
                                .get("description")
                                .cloned()
                                .or_else(|| {
                                    template
                                        .trim()
                                        .lines()
                                        .next()
                                        .filter(|line| line.starts_with('#'))
                                        .map(|line| line.trim_start_matches('#').trim().to_string())
                                })
                                .unwrap_or_default();

                            let model = frontmatter.get("model").cloned();
                            let agent = frontmatter.get("agent").cloned();
                            let subtask = frontmatter.get("subtask").is_some_and(|v| v == "true");

                            let file_name =
                                path.file_name().and_then(|n| n.to_str()).unwrap_or(&stem);

                            let source_label = format!("{}:{}", source, file_name);

                            // Project commands have higher priority (loaded first),
                            // so don't overwrite if already present.
                            commands.entry(stem.clone()).or_insert(CustomCommand {
                                name: stem,
                                template,
                                source: source_label,
                                description,
                                model,
                                agent,
                                subtask,
                            });
                        }
                        Err(e) => {
                            debug!("Failed to load command {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        if !commands.is_empty() {
            let names: Vec<&str> = commands.keys().map(|s| s.as_str()).collect();
            debug!("Loaded {} custom commands: {:?}", commands.len(), names);
        }

        self.commands = Some(commands);
        // SAFETY: we just set self.commands to Some on the line above
        self.commands
            .as_ref()
            .expect("commands was just set to Some")
    }

    /// Get a custom command by name.
    pub fn get_command(&mut self, name: &str) -> Option<&CustomCommand> {
        self.load_commands().get(name)
    }

    /// List all available custom commands.
    pub fn list_commands(&mut self) -> Vec<CommandInfo> {
        self.load_commands()
            .values()
            .map(|cmd| CommandInfo {
                name: cmd.name.clone(),
                description: cmd.description.clone(),
                source: cmd.source.clone(),
                model: cmd.model.clone(),
                agent: cmd.agent.clone(),
            })
            .collect()
    }

    /// Force reload of custom commands (clears cache).
    pub fn reload(&mut self) {
        self.commands = None;
    }

    /// Get command directories in priority order: project-local first, then global.
    ///
    /// Searches both `.opendev/commands/` and `.claude/commands/` at each level.
    fn get_command_dirs(&self) -> Vec<(PathBuf, &'static str)> {
        let mut dirs = Vec::new();

        // Project-local commands (highest priority)
        for subdir in &[".opendev/commands", ".claude/commands"] {
            let local = self.working_dir.join(subdir);
            if local.is_dir() {
                dirs.push((local, "project"));
            }
        }

        // User-global commands
        if let Some(home) = dirs_next::home_dir() {
            for subdir in &[".opendev/commands", ".claude/commands"] {
                let global = home.join(subdir);
                if global.is_dir() {
                    dirs.push((global, "global"));
                }
            }
        }

        dirs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_loader_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".opendev").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let commands = loader.load_commands();
        assert!(commands.is_empty());
    }

    #[test]
    fn test_loader_loads_md_files() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".opendev").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();
        fs::write(
            cmd_dir.join("review.md"),
            "# Code review\nReview $ARGUMENTS",
        )
        .unwrap();
        fs::write(cmd_dir.join("_hidden.md"), "should be skipped").unwrap();
        fs::write(cmd_dir.join(".secret.txt"), "should be skipped").unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let commands = loader.load_commands();
        assert_eq!(commands.len(), 1);
        let review = &commands["review"];
        assert_eq!(review.name, "review");
        assert_eq!(review.description, "Code review");
        assert!(review.source.contains("project:review.md"));
    }

    #[test]
    fn test_loader_caching_and_reload() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".opendev").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();
        fs::write(cmd_dir.join("hello.txt"), "Hello $ARGUMENTS").unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        assert_eq!(loader.load_commands().len(), 1);

        // Add another file — should still be cached
        fs::write(cmd_dir.join("bye.txt"), "Bye $ARGUMENTS").unwrap();
        assert_eq!(loader.load_commands().len(), 1);

        // After reload, picks up new file
        loader.reload();
        assert_eq!(loader.load_commands().len(), 2);
    }

    #[test]
    fn test_list_and_get_commands() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".opendev").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();
        fs::write(cmd_dir.join("greet"), "# Greet someone\nHi $1!").unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let list = loader.list_commands();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "greet");

        let cmd = loader.get_command("greet").unwrap();
        assert_eq!(cmd.expand("World", None), "# Greet someone\nHi World!");

        assert!(loader.get_command("nonexistent").is_none());
    }

    #[test]
    fn test_loader_frontmatter_metadata() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".opendev").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();
        fs::write(
            cmd_dir.join("commit.md"),
            "---\ndescription: Git commit\nmodel: gpt-4o\nagent: committer\nsubtask: true\n---\n\nCommit $ARGUMENTS",
        )
        .unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let cmd = loader.get_command("commit").unwrap();
        assert_eq!(cmd.description, "Git commit");
        assert_eq!(cmd.model.as_deref(), Some("gpt-4o"));
        assert_eq!(cmd.agent.as_deref(), Some("committer"));
        assert!(cmd.subtask);
        // Template should not contain frontmatter
        assert!(!cmd.template.contains("---"));
        assert!(cmd.template.contains("Commit $ARGUMENTS"));
    }

    #[test]
    fn test_frontmatter_description_overrides_hash() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".opendev").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();
        fs::write(
            cmd_dir.join("test.md"),
            "---\ndescription: From frontmatter\n---\n# From hash line\nBody",
        )
        .unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let cmd = loader.get_command("test").unwrap();
        assert_eq!(cmd.description, "From frontmatter");
    }

    // ── .claude/commands/ directory support ──

    #[test]
    fn test_loader_claude_commands_dir() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".claude").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();
        fs::write(cmd_dir.join("deploy.md"), "# Deploy\nDeploy $1").unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let commands = loader.load_commands();
        assert_eq!(commands.len(), 1);
        assert!(commands.contains_key("deploy"));
    }

    #[test]
    fn test_loader_opendev_overrides_claude() {
        let tmp = TempDir::new().unwrap();

        // Same command in both dirs
        let opendev_dir = tmp.path().join(".opendev").join("commands");
        let claude_dir = tmp.path().join(".claude").join("commands");
        fs::create_dir_all(&opendev_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            opendev_dir.join("review.md"),
            "# OpenDev review\nFrom opendev",
        )
        .unwrap();
        fs::write(claude_dir.join("review.md"), "# Claude review\nFrom claude").unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let cmd = loader.get_command("review").unwrap();
        // .opendev has higher priority (loaded first, or_insert prevents override)
        assert_eq!(cmd.description, "OpenDev review");
    }

    #[test]
    fn test_command_info_includes_model_agent() {
        let tmp = TempDir::new().unwrap();
        let cmd_dir = tmp.path().join(".opendev").join("commands");
        fs::create_dir_all(&cmd_dir).unwrap();
        fs::write(
            cmd_dir.join("smart.md"),
            "---\ndescription: Smart cmd\nmodel: claude-opus\nagent: researcher\n---\nDo $ARGUMENTS",
        )
        .unwrap();

        let mut loader = CustomCommandLoader::new(tmp.path());
        let list = loader.list_commands();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].model.as_deref(), Some("claude-opus"));
        assert_eq!(list[0].agent.as_deref(), Some("researcher"));
    }
}
