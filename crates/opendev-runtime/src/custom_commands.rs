//! Custom commands loaded from `.opendev/commands/` directory.
//!
//! Text files in the commands directory become slash commands. Supports:
//! - `$1`, `$2`, etc. for positional arguments
//! - `$ARGUMENTS` for all arguments
//! - Context variable substitution (`$KEY` → value)
//!
//! # Example
//!
//! `.opendev/commands/review.md` contains:
//! ```text
//! Review this code for: $ARGUMENTS
//! Focus on security and performance.
//! ```
//!
//! User types: `/review auth module`
//! Expands to:
//! ```text
//! Review this code for: auth module
//! Focus on security and performance.
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use tracing::debug;

/// A custom command loaded from a text file.
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// Command name (derived from filename).
    pub name: String,
    /// Template text with placeholder variables.
    pub template: String,
    /// Source identifier (e.g. "project:review.md").
    pub source: String,
    /// Human-readable description extracted from first `#` line.
    pub description: String,
}

impl CustomCommand {
    /// Expand the template with the given arguments and optional context.
    ///
    /// - Replaces `$ARGUMENTS` with the full argument string.
    /// - Replaces `$1`, `$2`, etc. with positional args (whitespace-split).
    /// - Cleans up unreplaced `$N` positional patterns.
    /// - Replaces context variables: `$KEY` → value.
    pub fn expand(&self, arguments: &str, context: Option<&HashMap<String, String>>) -> String {
        let mut result = self.template.replace("$ARGUMENTS", arguments);

        // Replace positional $1, $2, etc.
        let parts: Vec<&str> = if arguments.is_empty() {
            Vec::new()
        } else {
            arguments.split_whitespace().collect()
        };
        for (i, part) in parts.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            result = result.replace(&placeholder, part);
        }

        // Clean up unreplaced positional args
        let re = Regex::new(r"\$\d+").expect("valid regex");
        result = re.replace_all(&result, "").to_string();

        // Replace context variables
        if let Some(ctx) = context {
            for (key, value) in ctx {
                let placeholder = format!("${}", key.to_uppercase());
                result = result.replace(&placeholder, value);
            }
        }

        result.trim().to_string()
    }
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
    /// Scans `.opendev/commands/` under the project directory (higher priority)
    /// and then `~/.opendev/commands/` (global). Results are cached.
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
                        Ok(template) => {
                            // Extract description from first # line
                            let description = template
                                .trim()
                                .lines()
                                .next()
                                .filter(|line| line.starts_with('#'))
                                .map(|line| line.trim_start_matches('#').trim().to_string())
                                .unwrap_or_default();

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
            })
            .collect()
    }

    /// Force reload of custom commands (clears cache).
    pub fn reload(&mut self) {
        self.commands = None;
    }

    /// Get command directories in priority order: project-local first, then global.
    fn get_command_dirs(&self) -> Vec<(PathBuf, &'static str)> {
        let mut dirs = Vec::new();

        // Project-local commands (highest priority)
        let local = self.working_dir.join(".opendev").join("commands");
        if local.is_dir() {
            dirs.push((local, "project"));
        }

        // User-global commands
        if let Some(home) = dirs_next::home_dir() {
            let global = home.join(".opendev").join("commands");
            if global.is_dir() {
                dirs.push((global, "global"));
            }
        }

        dirs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_command(template: &str) -> CustomCommand {
        CustomCommand {
            name: "test".to_string(),
            template: template.to_string(),
            source: "test:test.md".to_string(),
            description: "Test command".to_string(),
        }
    }

    #[test]
    fn test_expand_arguments() {
        let cmd = make_command("Review this: $ARGUMENTS\nDone.");
        let result = cmd.expand("auth module", None);
        assert_eq!(result, "Review this: auth module\nDone.");
    }

    #[test]
    fn test_expand_positional_args() {
        let cmd = make_command("Fix $1 in $2");
        let result = cmd.expand("bug main.rs", None);
        assert_eq!(result, "Fix bug in main.rs");
    }

    #[test]
    fn test_expand_unreplaced_positional_cleaned() {
        let cmd = make_command("Use $1 and $2 and $3");
        let result = cmd.expand("foo bar", None);
        assert_eq!(result, "Use foo and bar and");
    }

    #[test]
    fn test_expand_empty_arguments() {
        let cmd = make_command("Hello $ARGUMENTS world");
        let result = cmd.expand("", None);
        assert_eq!(result, "Hello  world");
    }

    #[test]
    fn test_expand_context_variables() {
        let cmd = make_command("Check $FILE for $LANG issues");
        let mut ctx = HashMap::new();
        ctx.insert("file".to_string(), "main.rs".to_string());
        ctx.insert("lang".to_string(), "Rust".to_string());
        let result = cmd.expand("", Some(&ctx));
        assert_eq!(result, "Check main.rs for Rust issues");
    }

    #[test]
    fn test_expand_combined() {
        let cmd = make_command("Review $ARGUMENTS in $FILE focusing on $1");
        let mut ctx = HashMap::new();
        ctx.insert("file".to_string(), "lib.rs".to_string());
        let result = cmd.expand("security perf", Some(&ctx));
        assert_eq!(
            result,
            "Review security perf in lib.rs focusing on security"
        );
    }

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
}
