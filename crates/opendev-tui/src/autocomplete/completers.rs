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
            "autonomy" => vec![
                ("manual", "All commands require approval"),
                ("semi-auto", "Safe commands auto-approved"),
                ("auto", "All commands auto-approved"),
            ],
            // /models with no args opens the interactive picker — don't
            // autocomplete args so Enter submits the command directly.
            "models" => vec![],
            "model" | "session-models" => vec![
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
#[path = "completers_tests.rs"]
mod tests;
