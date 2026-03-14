//! Completer trait and concrete implementations.
//!
//! Each completer knows how to produce [`CompletionItem`]s for a given query
//! string. The [`AutocompleteEngine`](super::AutocompleteEngine) delegates to
//! the appropriate completer based on the detected trigger.

use std::path::PathBuf;

use super::file_finder::FileFinder;
use super::{CompletionItem, CompletionKind};
use crate::controllers::{BUILTIN_COMMANDS, SlashCommand};

// ── Completer trait ────────────────────────────────────────────────

/// Trait for types that can produce completion items for a query.
pub trait Completer {
    /// Return completions matching `query`.
    fn complete(&self, query: &str) -> Vec<CompletionItem>;
}

// ── CommandCompleter ───────────────────────────────────────────────

/// Completes slash commands from a registry.
pub struct CommandCompleter {
    /// Extra commands added at runtime (built-in ones are always included).
    extra_commands: Vec<SlashCommand>,
}

impl CommandCompleter {
    /// Create a new command completer.
    ///
    /// If `extra` is `Some`, those commands are added on top of the built-in
    /// set.
    pub fn new(extra: Option<&[SlashCommand]>) -> Self {
        Self {
            extra_commands: extra.map(|e| e.to_vec()).unwrap_or_default(),
        }
    }

    /// Add more commands to the completer.
    pub fn add_commands(&mut self, commands: &[SlashCommand]) {
        self.extra_commands.extend_from_slice(commands);
    }

    fn all_commands(&self) -> impl Iterator<Item = &SlashCommand> {
        BUILTIN_COMMANDS.iter().chain(self.extra_commands.iter())
    }
}

impl CommandCompleter {
    /// Provide argument completions for a specific slash command.
    ///
    /// For example, `/mode` suggests `plan` and `normal`, `/thinking` suggests
    /// thinking levels, `/models` or `/session-models` suggests model names.
    pub fn complete_args(&self, command: &str, query: &str) -> Vec<CompletionItem> {
        let candidates = match command {
            "mode" => vec![
                ("plan", "Read-only tools, planning mode"),
                ("normal", "Full tool access, normal mode"),
            ],
            "thinking" => vec![
                ("off", "Disable thinking"),
                ("low", "Basic reasoning"),
                ("medium", "Standard reasoning"),
                ("high", "Deep reasoning with critique"),
            ],
            "autonomy" => vec![
                ("manual", "All commands require approval"),
                ("semi-auto", "Safe commands auto-approved"),
                ("auto", "All commands auto-approved"),
            ],
            "model" | "models" | "session-models" => vec![
                ("gpt-4o", "OpenAI GPT-4o"),
                ("gpt-4o-mini", "OpenAI GPT-4o Mini"),
                ("claude-sonnet-4", "Anthropic Claude Sonnet 4"),
                ("claude-3-opus", "Anthropic Claude 3 Opus"),
                ("claude-3-haiku", "Anthropic Claude 3 Haiku"),
                ("gemini-1.5-pro", "Google Gemini 1.5 Pro"),
                ("deepseek-chat", "DeepSeek Chat"),
            ],
            "mcp" => vec![
                ("list", "List MCP servers"),
                ("add", "Add an MCP server"),
                ("remove", "Remove an MCP server"),
                ("enable", "Enable an MCP server"),
                ("disable", "Disable an MCP server"),
            ],
            "plugins" => vec![
                ("list", "List installed plugins"),
                ("install", "Install a plugin"),
                ("remove", "Remove a plugin"),
            ],
            "agents" => vec![("list", "List available agents")],
            "skills" => vec![("list", "List available skills")],
            _ => vec![],
        };

        let query_lower = query.to_lowercase();
        candidates
            .into_iter()
            .filter(|(name, _)| name.starts_with(&query_lower))
            .map(|(name, desc)| CompletionItem {
                insert_text: name.to_string(),
                label: name.to_string(),
                description: desc.to_string(),
                kind: CompletionKind::Command,
                score: 0.0,
            })
            .collect()
    }
}

impl Completer for CommandCompleter {
    fn complete(&self, query: &str) -> Vec<CompletionItem> {
        let query_lower = query.to_lowercase();
        self.all_commands()
            .filter(|cmd| cmd.name.starts_with(&query_lower))
            .map(|cmd| CompletionItem {
                insert_text: format!("/{}", cmd.name),
                label: format!("/{}", cmd.name),
                description: cmd.description.to_string(),
                kind: CompletionKind::Command,
                score: 0.0, // scored later by strategy
            })
            .collect()
    }
}

// ── FileCompleter ──────────────────────────────────────────────────

/// Completes file paths relative to a working directory.
///
/// Uses [`FileFinder`] for gitignore-aware file discovery.
pub struct FileCompleter {
    finder: FileFinder,
}

impl FileCompleter {
    /// Create a new file completer rooted at `working_dir`.
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            finder: FileFinder::new(working_dir),
        }
    }
}

impl Completer for FileCompleter {
    fn complete(&self, query: &str) -> Vec<CompletionItem> {
        let paths = self.finder.find_files(query, 50);
        paths
            .into_iter()
            .map(|rel| {
                let is_dir = self.finder.working_dir().join(&rel).is_dir();
                let display = if is_dir {
                    format!("{}/", rel.display())
                } else {
                    rel.display().to_string()
                };
                CompletionItem {
                    insert_text: format!("@{}", display),
                    label: display,
                    description: if is_dir {
                        "dir".to_string()
                    } else {
                        super::formatters::CompletionFormatter::file_size_string(
                            &self.finder.working_dir().join(&rel),
                        )
                    },
                    kind: CompletionKind::File,
                    score: 0.0,
                }
            })
            .collect()
    }
}

// ── SymbolCompleter ────────────────────────────────────────────────

/// Placeholder completer for code symbols.
///
/// In a full implementation this would query an LSP server or a tag index.
/// For now it returns an empty list.
pub struct SymbolCompleter {
    symbols: Vec<(String, String)>, // (name, kind)
}

impl SymbolCompleter {
    /// Create a new (empty) symbol completer.
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
        }
    }

    /// Register known symbols for completion.
    pub fn register_symbols(&mut self, symbols: Vec<(String, String)>) {
        self.symbols = symbols;
    }
}

impl Default for SymbolCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer for SymbolCompleter {
    fn complete(&self, query: &str) -> Vec<CompletionItem> {
        let query_lower = query.to_lowercase();
        self.symbols
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
            .map(|(name, kind)| CompletionItem {
                insert_text: name.clone(),
                label: name.clone(),
                description: kind.clone(),
                kind: CompletionKind::Symbol,
                score: 0.0,
            })
            .collect()
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_completer_basic() {
        let c = CommandCompleter::new(None);
        let results = c.complete("hel");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].insert_text, "/help");
        assert_eq!(results[0].kind, CompletionKind::Command);
    }

    #[test]
    fn test_command_completer_empty_query() {
        let c = CommandCompleter::new(None);
        let results = c.complete("");
        // Should return all built-in commands
        assert_eq!(results.len(), BUILTIN_COMMANDS.len());
    }

    #[test]
    fn test_command_completer_no_match() {
        let c = CommandCompleter::new(None);
        let results = c.complete("zzzzz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_command_completer_extra_commands() {
        let extra = vec![SlashCommand {
            name: "custom",
            description: "a custom command",
        }];
        let c = CommandCompleter::new(Some(&extra));
        let results = c.complete("cust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].insert_text, "/custom");
    }

    #[test]
    fn test_command_completer_add_commands() {
        let mut c = CommandCompleter::new(None);
        let before = c.complete("").len();
        c.add_commands(&[SlashCommand {
            name: "newcmd",
            description: "new",
        }]);
        let after = c.complete("").len();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn test_symbol_completer_empty() {
        let c = SymbolCompleter::new();
        let results = c.complete("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_symbol_completer_with_symbols() {
        let mut c = SymbolCompleter::new();
        c.register_symbols(vec![
            ("MyStruct".to_string(), "struct".to_string()),
            ("my_function".to_string(), "fn".to_string()),
            ("MyEnum".to_string(), "enum".to_string()),
        ]);
        // "My" matches all three case-insensitively: MyStruct, my_function, MyEnum
        let results = c.complete("My");
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.kind == CompletionKind::Symbol));

        // "MyS" should only match MyStruct
        let results2 = c.complete("MyS");
        assert_eq!(results2.len(), 1);
        assert!(results2[0].label.contains("MyStruct"));
    }

    #[test]
    fn test_file_completer_in_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Create a test file
        std::fs::write(dir.path().join("hello.txt"), "content").unwrap();
        let c = FileCompleter::new(dir.path().to_path_buf());
        let results = c.complete("hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, CompletionKind::File);
        assert!(results[0].label.contains("hello.txt"));
    }

    // --- Argument completion tests ---

    #[test]
    fn test_arg_completion_mode() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("mode", "");
        assert_eq!(results.len(), 2);
        let labels: Vec<&str> = results.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"plan"));
        assert!(labels.contains(&"normal"));
    }

    #[test]
    fn test_arg_completion_mode_prefix() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("mode", "pl");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "plan");
    }

    #[test]
    fn test_arg_completion_thinking() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("thinking", "");
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_arg_completion_autonomy() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("autonomy", "");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_arg_completion_model_names() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("model", "claude");
        assert!(results.len() >= 2);
        for r in &results {
            assert!(r.label.starts_with("claude"));
        }
    }

    #[test]
    fn test_arg_completion_mcp() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("mcp", "");
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_arg_completion_unknown_command() {
        let c = CommandCompleter::new(None);
        let results = c.complete_args("nonexistent", "");
        assert!(results.is_empty());
    }
}
