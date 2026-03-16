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

/// Parse YAML frontmatter delimited by `---` lines.
///
/// Returns `(frontmatter_map, body)` where frontmatter_map contains
/// key-value pairs and body is the content after the closing `---`.
/// If no frontmatter is present, returns empty map and full content.
fn parse_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (HashMap::new(), content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let after_first = after_first.trim_start_matches(['\r', '\n']);

    if let Some(end_pos) = after_first.find("\n---") {
        let fm_text = &after_first[..end_pos];
        let body_start = end_pos + 4; // skip \n---
        let body = after_first[body_start..].trim_start_matches(['\r', '\n']);

        let mut map = HashMap::new();
        for line in fm_text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_string();
                let value = value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                map.insert(key, value);
            }
        }

        (map, body.to_string())
    } else {
        // No closing ---, treat as no frontmatter
        (HashMap::new(), content.to_string())
    }
}

/// Execute shell command substitutions in the template.
///
/// Replaces `!`cmd`` patterns with the stdout of the command.
/// Failures are replaced with `[error: ...]` inline.
fn expand_shell_commands(content: &str) -> String {
    let re = Regex::new(r"!`([^`]+)`").expect("valid regex");
    re.replace_all(content, |caps: &regex::Captures| {
        let cmd = &caps[1];
        match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                format!("[error: {cmd}: {}]", stderr.trim())
            }
            Err(e) => format!("[error: {cmd}: {e}]"),
        }
    })
    .to_string()
}

impl CustomCommand {
    /// Expand the template with the given arguments and optional context.
    ///
    /// - Executes `!`cmd`` shell substitutions.
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

        // Execute shell command substitutions last
        result = expand_shell_commands(&result);

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

    fn make_command(template: &str) -> CustomCommand {
        CustomCommand {
            name: "test".to_string(),
            template: template.to_string(),
            source: "test:test.md".to_string(),
            description: "Test command".to_string(),
            model: None,
            agent: None,
            subtask: false,
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

    // ── Frontmatter parsing ──

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\ndescription: Code review\nmodel: gpt-4o\n---\n\nReview $ARGUMENTS";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("description").unwrap(), "Code review");
        assert_eq!(fm.get("model").unwrap(), "gpt-4o");
        assert_eq!(body.trim(), "Review $ARGUMENTS");
    }

    #[test]
    fn test_parse_frontmatter_quoted_values() {
        let content = "---\ndescription: \"Commit and push\"\nagent: 'reviewer'\n---\nBody";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("description").unwrap(), "Commit and push");
        assert_eq!(fm.get("agent").unwrap(), "reviewer");
        assert_eq!(body.trim(), "Body");
    }

    #[test]
    fn test_parse_frontmatter_none() {
        let content = "# No frontmatter\nJust a template";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_frontmatter_no_closing() {
        let content = "---\nkey: value\nno closing delimiter";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
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

    // ── Shell substitution ──

    #[test]
    fn test_expand_shell_command() {
        let cmd = make_command("Hello !`echo world`");
        let result = cmd.expand("", None);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_expand_shell_command_with_args() {
        let cmd = make_command("Version: !`echo 1.2.3`");
        let result = cmd.expand("", None);
        assert_eq!(result, "Version: 1.2.3");
    }

    #[test]
    fn test_expand_shell_command_failure() {
        let cmd = make_command("Result: !`false`");
        let result = cmd.expand("", None);
        assert!(result.starts_with("Result: [error:"));
    }

    #[test]
    fn test_expand_shell_no_substitution() {
        let cmd = make_command("No shell here");
        let result = cmd.expand("", None);
        assert_eq!(result, "No shell here");
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
