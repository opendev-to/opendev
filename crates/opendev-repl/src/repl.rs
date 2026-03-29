//! Main REPL loop: read input -> process -> display.
//!
//! Mirrors `opendev/repl/repl.py`.

use std::io::{self, BufRead, Write};

use tracing::{info, warn};

use opendev_history::SessionManager;
use opendev_runtime::AutonomyLevel;
use opendev_tools_core::ToolRegistry;

use crate::commands::{BuiltinCommands, CommandOutcome};
use crate::error::ReplError;
use crate::query_processor::QueryProcessor;

/// Operation mode for the REPL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationMode {
    /// Normal mode — full tool access.
    Normal,
    /// Plan mode — read-only tools only.
    Plan,
}

impl std::fmt::Display for OperationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationMode::Normal => write!(f, "NORMAL"),
            OperationMode::Plan => write!(f, "PLAN"),
        }
    }
}

/// State shared across the REPL session.
pub struct ReplState {
    /// Current operation mode.
    pub mode: OperationMode,
    /// Current autonomy level.
    pub autonomy_level: AutonomyLevel,
    /// Whether the REPL is running.
    pub running: bool,
    /// Last user prompt (for context display).
    pub last_prompt: String,
    /// Last operation summary.
    pub last_operation_summary: String,
    /// Last error message (if any).
    pub last_error: Option<String>,
    /// Last LLM latency in milliseconds.
    pub last_latency_ms: Option<u64>,
    /// Whether a plan-mode query is pending (via Shift+Tab toggle).
    pub pending_plan_request: bool,
    /// Flag set by /clear command; REPL loop consumes and clears session messages.
    pub messages_cleared: bool,
    /// Flag set by /compact command; REPL loop consumes and triggers compaction.
    pub compact_requested: bool,
    /// Prompt set by /init command; REPL loop consumes and processes it.
    pub init_prompt: Option<String>,
}

impl Default for ReplState {
    fn default() -> Self {
        Self {
            mode: OperationMode::Normal,
            autonomy_level: AutonomyLevel::default(),
            running: true,
            last_prompt: String::new(),
            last_operation_summary: String::from("—"),
            last_error: None,
            last_latency_ms: None,
            pending_plan_request: false,
            messages_cleared: false,
            compact_requested: false,
            init_prompt: None,
        }
    }
}

/// Interactive REPL for AI-powered coding assistance.
///
/// Orchestrates reading user input, dispatching slash commands,
/// and processing AI queries via the ReAct loop.
pub struct Repl {
    /// Shared REPL state.
    pub state: ReplState,
    /// Session manager for conversation persistence.
    session_manager: SessionManager,
    /// Tool registry for executing tools.
    tool_registry: ToolRegistry,
    /// Query processor for AI interactions.
    query_processor: QueryProcessor,
    /// Built-in command handler.
    commands: BuiltinCommands,
}

impl Repl {
    /// Create a new REPL instance.
    pub fn new(session_manager: SessionManager, tool_registry: ToolRegistry) -> Self {
        let query_processor = QueryProcessor::new();
        let commands = BuiltinCommands::new();
        Self {
            state: ReplState::default(),
            session_manager,
            tool_registry,
            query_processor,
            commands,
        }
    }

    /// Set an initial message to be processed when the REPL starts.
    ///
    /// This message will be processed as a user query before entering
    /// the interactive input loop.
    pub fn set_initial_message(&mut self, message: String) {
        self.state.last_prompt = message;
    }

    /// Run the REPL loop.
    ///
    /// Reads lines from stdin, dispatches commands or processes queries,
    /// and loops until the user exits.
    pub async fn run(&mut self) -> Result<(), ReplError> {
        info!("Starting REPL");
        self.print_welcome();

        // Process initial message if one was set via set_initial_message()
        if !self.state.last_prompt.is_empty() {
            let initial = self.state.last_prompt.clone();
            info!(message = %initial, "Processing initial message");
            self.process_query(&initial).await?;
        }

        let stdin = io::stdin();
        let mut reader = stdin.lock();

        while self.state.running {
            self.print_prompt();

            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(error = %e, "Error reading input");
                    return Err(ReplError::Io(e));
                }
            }

            let input = line.trim();
            if input.is_empty() {
                continue;
            }

            if input.starts_with('/') {
                self.handle_command(input);

                // Consume flags set by commands
                if self.state.messages_cleared {
                    self.state.messages_cleared = false;
                    if let Some(session) = self.session_manager.current_session_mut() {
                        session.messages.clear();
                    }
                }
                if self.state.compact_requested {
                    self.state.compact_requested = false;
                    // Compaction will be driven by ContextCompactor when wired up.
                    info!("Compact flag consumed; compaction will run on next query.");
                }

                if let Some(query) = self.state.init_prompt.take() {
                    self.state.last_prompt = query.clone();
                    self.process_query(&query).await?;
                }

                continue;
            }

            self.state.last_prompt = input.to_string();
            self.process_query(input).await?;
        }

        self.cleanup();
        Ok(())
    }

    /// Print the welcome banner.
    fn print_welcome(&self) {
        println!("OpenDev -- AI-powered coding assistant");
        println!("Type /help for commands, /exit to quit.");
        println!(
            "Mode: {} | Autonomy: {}",
            self.state.mode, self.state.autonomy_level
        );
        println!();
    }

    /// Print the input prompt.
    fn print_prompt(&self) {
        let mode_indicator = match self.state.mode {
            OperationMode::Normal => ">",
            OperationMode::Plan => "plan>",
        };
        print!("{} ", mode_indicator);
        let _ = io::stdout().flush();
    }

    /// Handle a slash command.
    fn handle_command(&mut self, input: &str) {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let args = parts.get(1).copied().unwrap_or("");

        match self.commands.dispatch(&cmd, args, &mut self.state) {
            CommandOutcome::Handled => {}
            CommandOutcome::Exit => {
                self.state.running = false;
            }
            CommandOutcome::Unknown => {
                eprintln!("Unknown command: {}", cmd);
                eprintln!("Type /help for available commands");
            }
        }
    }

    /// Process a user query through the AI pipeline.
    async fn process_query(&mut self, query: &str) -> Result<(), ReplError> {
        let plan_requested = self.state.pending_plan_request;
        if plan_requested {
            self.state.pending_plan_request = false;
        }

        let result = self
            .query_processor
            .process(
                query,
                &mut self.session_manager,
                &self.tool_registry,
                plan_requested,
            )
            .await?;

        self.state.last_operation_summary = result.operation_summary;
        self.state.last_error = result.error;
        self.state.last_latency_ms = result.latency_ms;

        // Print the assistant response
        if !result.content.is_empty() {
            println!("{}", result.content);
        }

        Ok(())
    }

    /// Clean up resources on exit.
    fn cleanup(&mut self) {
        info!("Cleaning up REPL resources");

        // Persist mode settings into session metadata before saving
        self.session_manager
            .set_metadata("mode", &self.state.mode.to_string());
        self.session_manager
            .set_metadata("autonomy_level", &self.state.autonomy_level.to_string());

        if let Err(e) = self.session_manager.save_current() {
            warn!(error = %e, "Failed to save session on exit");
        }
        println!("Goodbye!");
    }
}

#[cfg(test)]
#[path = "repl_tests.rs"]
mod tests;
