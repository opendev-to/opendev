//! Slash command registry and autocomplete.
//!
//! Mirrors Python `CommandRegistry` and `BUILTIN_COMMANDS` — provides the
//! full set of slash commands available in the TUI with autocomplete support.

/// A registered slash command.
#[derive(Debug, Clone)]
pub struct SlashCommand {
    /// Command name (without the leading `/`).
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
}

/// All built-in slash commands.
///
/// Mirrors the Python `BUILTIN_COMMANDS` registry exactly.
pub const BUILTIN_COMMANDS: &[SlashCommand] = &[
    // Session management
    SlashCommand {
        name: "help",
        description: "show available commands and help",
    },
    SlashCommand {
        name: "exit",
        description: "exit OpenDev",
    },
    SlashCommand {
        name: "quit",
        description: "exit OpenDev (alias for /exit)",
    },
    SlashCommand {
        name: "clear",
        description: "clear current session and start fresh",
    },
    SlashCommand {
        name: "models",
        description: "interactive model/provider selector (global)",
    },
    SlashCommand {
        name: "session-models",
        description: "set model for this session only",
    },
    // Execution commands
    SlashCommand {
        name: "mode",
        description: "switch between NORMAL and PLAN mode",
    },
    // Advanced commands
    SlashCommand {
        name: "init",
        description: "analyze codebase and generate AGENTS.md",
    },
    SlashCommand {
        name: "mcp",
        description: "manage MCP servers and tools",
    },
    // Background task management
    SlashCommand {
        name: "tasks",
        description: "list background tasks",
    },
    SlashCommand {
        name: "task",
        description: "show output from a background task (usage: /task <id>)",
    },
    SlashCommand {
        name: "kill",
        description: "kill a background task (usage: /kill <id>)",
    },
    // Agent and skill management
    SlashCommand {
        name: "agents",
        description: "create and manage custom agents",
    },
    SlashCommand {
        name: "skills",
        description: "create and manage custom skills with AI assistance",
    },
    SlashCommand {
        name: "plugins",
        description: "manage plugins and marketplaces",
    },
    // Autonomy
    SlashCommand {
        name: "autonomy",
        description: "set autonomy level (manual/semi-auto/auto)",
    },
    // Utility commands
    SlashCommand {
        name: "sound",
        description: "play a test notification sound",
    },
    SlashCommand {
        name: "compact",
        description: "manually compact conversation context",
    },
    // Undo/Redo/Share
    SlashCommand {
        name: "undo",
        description: "undo last file changes",
    },
    SlashCommand {
        name: "redo",
        description: "redo undone changes",
    },
    SlashCommand {
        name: "share",
        description: "share session as HTML",
    },
    // Session management
    SlashCommand {
        name: "sessions",
        description: "list saved sessions",
    },
    // Background agents
    SlashCommand {
        name: "bg",
        description: "manage background agents",
    },
];

/// Find commands matching a query prefix.
///
/// The query should be the text after `/` (e.g., for `/he`, pass `"he"`).
pub fn find_matching_commands(query: &str) -> Vec<&'static SlashCommand> {
    let query_lower = query.to_lowercase();
    BUILTIN_COMMANDS
        .iter()
        .filter(|cmd| cmd.name.starts_with(&query_lower))
        .collect()
}

/// Check if a command exists by exact name.
pub fn is_command(name: &str) -> bool {
    BUILTIN_COMMANDS.iter().any(|cmd| cmd.name == name)
}

#[cfg(test)]
#[path = "slash_commands_tests.rs"]
mod tests;
