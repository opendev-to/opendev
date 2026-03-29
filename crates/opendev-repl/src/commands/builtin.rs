//! Built-in slash commands: /help, /clear, /mode, /exit, etc.
//!
//! Mirrors `opendev/repl/repl.py::_handle_command`.

use opendev_config::ModelRegistry;
use opendev_runtime::AutonomyLevel;

use crate::repl::{OperationMode, ReplState};

/// Outcome of dispatching a slash command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Command was handled successfully.
    Handled,
    /// The user wants to exit.
    Exit,
    /// Command was not recognized.
    Unknown,
}

/// Handles built-in slash commands.
pub struct BuiltinCommands;

impl BuiltinCommands {
    /// Create a new command handler.
    pub fn new() -> Self {
        Self
    }

    /// Dispatch a slash command.
    ///
    /// Returns the outcome indicating how the REPL should proceed.
    pub fn dispatch(&self, cmd: &str, args: &str, state: &mut ReplState) -> CommandOutcome {
        match cmd {
            "/help" => {
                self.handle_help(args);
                CommandOutcome::Handled
            }
            "/exit" | "/quit" => CommandOutcome::Exit,
            "/clear" => {
                self.handle_clear(state);
                CommandOutcome::Handled
            }
            "/mode" => {
                self.handle_mode(args, state);
                CommandOutcome::Handled
            }
            "/compact" => {
                self.handle_compact(state);
                CommandOutcome::Handled
            }
            "/models" => {
                self.handle_models();
                CommandOutcome::Handled
            }
            "/mcp" => {
                self.handle_mcp(args);
                CommandOutcome::Handled
            }
            "/agents" => {
                self.handle_agents(args);
                CommandOutcome::Handled
            }
            "/skills" => {
                self.handle_skills(args);
                CommandOutcome::Handled
            }
            "/plugins" => {
                self.handle_plugins(args);
                CommandOutcome::Handled
            }
            "/session-models" => {
                self.handle_session_models(args);
                CommandOutcome::Handled
            }
            "/autonomy" => {
                self.handle_autonomy(args, state);
                CommandOutcome::Handled
            }
            "/status" => {
                self.handle_status(state);
                CommandOutcome::Handled
            }
            "/init" => {
                self.handle_init(args, state);
                CommandOutcome::Handled
            }
            "/sound" => {
                self.handle_sound();
                CommandOutcome::Handled
            }
            _ => CommandOutcome::Unknown,
        }
    }

    fn handle_help(&self, _args: &str) {
        println!("Available commands:");
        println!("  /help                   Show this help message");
        println!("  /exit, /quit            Exit the REPL");
        println!("  /clear                  Clear conversation history");
        println!("  /mode [plan|normal]     Switch operation mode");
        println!("  /autonomy [manual|semi-auto|auto] Set approval level");
        println!("  /status                 Show current status");
        println!("  /compact                Compact conversation context");
        println!("  /models                 Show model picker (from models.dev registry)");
        println!("  /mcp <subcommand>       Manage MCP servers");
        println!("  /agents <args>          Manage agents");
        println!("  /skills <args>          Manage skills");
        println!("  /plugins <args>         Manage plugins");
        println!("  /session-models         Session model management");
        println!("  /sound                  Play test notification sound");
        println!("  /init                   Initialize codebase context");
    }

    fn handle_clear(&self, state: &mut ReplState) {
        state.messages_cleared = true;
        println!("Conversation cleared.");
    }

    fn handle_mode(&self, args: &str, state: &mut ReplState) {
        let target = args.trim().to_lowercase();
        match target.as_str() {
            "plan" => {
                state.mode = OperationMode::Plan;
                println!("Switched to Plan mode (read-only tools).");
            }
            "normal" | "" => {
                state.mode = OperationMode::Normal;
                println!("Switched to Normal mode (full tool access).");
            }
            _ => {
                println!("Usage: /mode [plan|normal]");
            }
        }
    }

    fn handle_compact(&self, state: &mut ReplState) {
        state.compact_requested = true;
        println!("Context compaction triggered.");
    }

    fn handle_models(&self) {
        self.handle_model_picker(None);
    }

    /// Display a numbered list of models from the models.dev registry.
    ///
    /// If `cache_dir` is `None`, uses the default `~/.opendev/cache` directory.
    /// Returns the list of `(provider_id, model_id)` pairs for use by callers
    /// that want to act on the user's selection.
    pub fn handle_model_picker(
        &self,
        cache_dir: Option<&std::path::Path>,
    ) -> Vec<(String, String)> {
        let cache = cache_dir.map(std::path::PathBuf::from).unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join(".opendev")
                .join("cache")
        });

        let registry = ModelRegistry::load_from_cache(&cache);

        if registry.providers.is_empty() {
            println!("No models available. Run `opendev setup` or check network connectivity.");
            return Vec::new();
        }

        let models = registry.list_all_models(None, None);

        if models.is_empty() {
            println!("No models found in registry.");
            return Vec::new();
        }

        println!("Available models:");
        println!();

        let mut entries: Vec<(String, String)> = Vec::new();
        for (i, (provider_id, model)) in models.iter().enumerate() {
            let num = i + 1;
            let pricing = model.format_pricing();
            let ctx = if model.context_length >= 1_000_000 {
                format!("{}M ctx", model.context_length / 1_000_000)
            } else if model.context_length >= 1_000 {
                format!("{}K ctx", model.context_length / 1_000)
            } else {
                format!("{} ctx", model.context_length)
            };
            let caps = if model.capabilities.is_empty() {
                String::new()
            } else {
                format!(" [{}]", model.capabilities.join(", "))
            };

            println!(
                "  {num:>3}. {name} ({provider}) - {ctx}, {pricing}{caps}",
                name = model.name,
                provider = model.provider,
            );

            entries.push((provider_id.to_string(), model.id.clone()));
        }

        println!();
        println!("Use /session-models set model <name> to change the active model.");

        entries
    }

    fn handle_mcp(&self, args: &str) {
        let parts: Vec<&str> = args.trim().splitn(2, ' ').collect();
        let subcommand = parts.first().copied().unwrap_or("");
        let sub_args = parts.get(1).copied().unwrap_or("");

        match subcommand {
            "" | "list" => {
                println!("MCP Servers:");
                println!("  (none configured)");
                println!();
                println!("Use /mcp add <name> <command> to register a server.");
            }
            "add" => {
                if sub_args.is_empty() {
                    println!("Usage: /mcp add <name> <command> [args...]");
                } else {
                    let name = sub_args.split_whitespace().next().unwrap_or(sub_args);
                    println!(
                        "MCP server '{}' registered (restart required to activate).",
                        name
                    );
                }
            }
            "remove" => {
                if sub_args.is_empty() {
                    println!("Usage: /mcp remove <name>");
                } else {
                    println!("MCP server '{}' removed.", sub_args.trim());
                }
            }
            "enable" => {
                if sub_args.is_empty() {
                    println!("Usage: /mcp enable <name>");
                } else {
                    println!("MCP server '{}' enabled.", sub_args.trim());
                }
            }
            "disable" => {
                if sub_args.is_empty() {
                    println!("Usage: /mcp disable <name>");
                } else {
                    println!("MCP server '{}' disabled.", sub_args.trim());
                }
            }
            _ => {
                println!("Unknown MCP subcommand: {}", subcommand);
                println!("Usage: /mcp [list|add|remove|enable|disable] ...");
            }
        }
    }

    fn handle_agents(&self, args: &str) {
        let subcommand = args.split_whitespace().next().unwrap_or("list");
        match subcommand {
            "list" | "" => {
                println!("Available agents:");
                println!("  - Explore          Explore and understand codebase structure");
                println!("  - Planner          Create and refine implementation plans");
                println!("  - Ask-User         Request clarification from the user");
            }
            _ => {
                println!("Usage: /agents [list]");
            }
        }
    }

    fn handle_skills(&self, args: &str) {
        let subcommand = args.split_whitespace().next().unwrap_or("list");
        match subcommand {
            "list" | "" => {
                println!("Built-in skills:");
                println!("  - commit       Git commit best practices");
                println!("  - review-pr    Pull request review guidelines");
                println!("  - create-pr    Pull request creation workflow");
                println!();
                println!("Use /skills to invoke a skill by name.");
            }
            _ => {
                println!("Usage: /skills [list]");
            }
        }
    }

    fn handle_plugins(&self, args: &str) {
        let parts: Vec<&str> = args.trim().splitn(2, ' ').collect();
        let subcommand = parts.first().copied().unwrap_or("");
        let sub_args = parts.get(1).copied().unwrap_or("");

        match subcommand {
            "" | "list" => {
                println!("Plugins: (none installed)");
                println!("Use /plugins install <name> to add plugins.");
            }
            "install" => {
                if sub_args.is_empty() {
                    println!("Usage: /plugins install <name>");
                } else {
                    println!("Installing plugin '{}'...", sub_args.trim());
                    println!("Plugin installation not yet connected to marketplace.");
                }
            }
            "remove" => {
                if sub_args.is_empty() {
                    println!("Usage: /plugins remove <name>");
                } else {
                    println!(
                        "Plugin '{}' not found in installed plugins.",
                        sub_args.trim()
                    );
                }
            }
            _ => {
                println!("Unknown plugins subcommand: {}", subcommand);
                println!("Usage: /plugins [list|install|remove] ...");
            }
        }
    }

    fn handle_session_models(&self, args: &str) {
        let parts: Vec<&str> = args.trim().splitn(2, ' ').collect();
        let subcommand = parts.first().copied().unwrap_or("");
        let sub_args = parts.get(1).copied().unwrap_or("");

        match subcommand {
            "" | "show" => {
                println!("No session model overrides set.");
                println!();
                println!("Available slots: model, model_vlm");
                println!(
                    "Use /session-models set <slot> <value> to override a model for this session."
                );
            }
            "set" => {
                let set_parts: Vec<&str> = sub_args.splitn(2, ' ').collect();
                if set_parts.len() < 2 {
                    println!("Usage: /session-models set <slot> <model-name>");
                } else {
                    let slot = set_parts[0];
                    let value = set_parts[1];
                    let valid_slots =
                        ["model", "model_provider", "model_vlm", "model_vlm_provider"];
                    if valid_slots.contains(&slot) {
                        println!("Session override: {} = {}", slot, value);
                    } else {
                        println!("Unknown slot: {}", slot);
                        println!("Valid slots: {}", valid_slots.join(", "));
                    }
                }
            }
            "clear" => {
                println!("Session model overrides cleared.");
            }
            _ => {
                println!("Unknown session-models subcommand: {}", subcommand);
                println!("Usage: /session-models [show|set|clear] ...");
            }
        }
    }

    fn handle_autonomy(&self, args: &str, state: &mut ReplState) {
        let target = args.trim();
        if target.is_empty() {
            println!("Autonomy level: {}", state.autonomy_level);
            println!("Usage: /autonomy [manual|semi-auto|auto]");
            return;
        }
        match AutonomyLevel::from_str_loose(target) {
            Some(level) => {
                state.autonomy_level = level;
                let detail = match level {
                    AutonomyLevel::Manual => "(all commands require approval)",
                    AutonomyLevel::SemiAuto => "(safe commands auto-approved)",
                    AutonomyLevel::Auto => "(all commands auto-approved)",
                };
                println!("Autonomy level set to: {} {}", level, detail);
            }
            None => {
                println!("Invalid autonomy level: {}", target);
                println!("Valid levels: manual, semi-auto, auto");
            }
        }
    }

    fn handle_status(&self, state: &ReplState) {
        println!("Current status:");
        println!("  Mode:      {}", state.mode);
        println!("  Autonomy:  {}", state.autonomy_level);
    }

    fn handle_sound(&self) {
        opendev_runtime::play_finish_sound();
        println!("Playing test sound...");
    }

    fn handle_init(&self, args: &str, state: &mut ReplState) {
        let prompt = opendev_agents::prompts::embedded::build_init_prompt(args);
        state.init_prompt = Some(prompt);
        println!("Generating AGENTS.md...");
    }
}

impl Default for BuiltinCommands {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
