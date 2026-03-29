//! Slash command execution: /mode, /models, /help, etc.

use crate::event::AppEvent;

use super::{App, AutonomyLevel, DisplayMessage, DisplayRole, OperationMode};

impl App {
    pub(super) fn push_system_message(&mut self, content: String) {
        self.state
            .messages
            .push(DisplayMessage::new(DisplayRole::System, content));
        self.state.message_generation += 1;
    }

    /// Push a slash command echo line (e.g. `❯ /mode plan`).
    pub(super) fn push_slash_echo(&mut self, cmd: &str) {
        self.state
            .messages
            .push(DisplayMessage::new(DisplayRole::SlashCommand, cmd));
    }

    /// Push a command result line that attaches below the echo.
    pub(super) fn push_command_result(&mut self, content: String) {
        self.state
            .messages
            .push(DisplayMessage::new(DisplayRole::CommandResult, content));
        self.state.message_generation += 1;
    }

    /// Execute a slash command locally.
    pub(super) fn execute_slash_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd[1..].splitn(2, ' ').collect();
        let name = parts[0];
        let args = parts.get(1).map(|s| s.trim());

        match name {
            "exit" | "quit" | "q" => {
                self.state.running = false;
            }
            "clear" => {
                self.state.messages.clear();
                self.state.scroll_offset = 0;
                self.state.user_scrolled = false;
                self.state.message_generation += 1;
            }
            "mode" => {
                self.push_slash_echo(cmd);
                match args {
                    Some(arg) => {
                        if let Some(mode) = OperationMode::from_str_loose(arg) {
                            self.state.mode = mode;
                        } else {
                            self.push_command_result(format!(
                                "Unknown mode '{arg}'. Use: normal, plan"
                            ));
                            return;
                        }
                    }
                    None => {
                        self.state.mode = match self.state.mode {
                            OperationMode::Normal => OperationMode::Plan,
                            OperationMode::Plan => OperationMode::Normal,
                        };
                    }
                }
                self.push_command_result(format!("Mode set to {}", self.state.mode));
            }
            "autonomy" => {
                self.push_slash_echo(cmd);
                match args {
                    Some(arg) => {
                        if let Some(level) = AutonomyLevel::from_str_loose(arg) {
                            self.state.autonomy = level;
                        } else {
                            self.push_command_result(format!(
                                "Unknown autonomy level '{arg}'. Use: manual, semi-auto, auto"
                            ));
                            return;
                        }
                    }
                    None => {
                        self.state.autonomy = match self.state.autonomy {
                            AutonomyLevel::Manual => AutonomyLevel::SemiAuto,
                            AutonomyLevel::SemiAuto => AutonomyLevel::Auto,
                            AutonomyLevel::Auto => AutonomyLevel::Manual,
                        };
                    }
                }
                self.push_command_result(format!("Autonomy set to {}", self.state.autonomy));
            }
            "models" => {
                // Always open interactive model picker
                let cache_dir = opendev_config::Paths::new(None).global_cache_dir();
                let picker = crate::controllers::ModelPickerController::from_registry(
                    &cache_dir,
                    &self.state.model,
                );
                if picker.filtered_count() == 0 {
                    self.push_slash_echo(cmd);
                    self.push_command_result(
                        "No models available. Run 'opendev setup' to configure providers."
                            .to_string(),
                    );
                } else {
                    self.model_picker_controller = Some(picker);
                }
            }
            "session-models" => {
                self.push_slash_echo(cmd);
                match args {
                    Some("clear") => {
                        self.push_command_result("Session model override cleared".to_string());
                    }
                    Some(model_name) if !model_name.is_empty() => {
                        self.state.model = model_name.to_string();
                        self.push_command_result(format!(
                            "Session model set to {}",
                            self.state.model
                        ));
                        if let Some(ref tx) = self.user_message_tx {
                            let _ = tx.send(format!("\x00__MODEL_CHANGE__{}", self.state.model));
                        }
                    }
                    _ => {
                        self.push_command_result(format!(
                            "Current model: {}. Usage: /session-models <name>",
                            self.state.model
                        ));
                    }
                }
            }
            "mcp" => {
                self.push_slash_echo(cmd);
                let result = self.mcp_controller.handle_command(args.unwrap_or(""));
                self.push_command_result(result);
            }
            "tasks" => {
                self.push_slash_echo(cmd);
                let msg = if let Ok(mgr) = self.task_manager.try_lock() {
                    let tasks = mgr.all_tasks();
                    if tasks.is_empty() {
                        "No background tasks".to_string()
                    } else {
                        let mut lines = vec![format!(
                            "Background tasks ({} total, {} running):",
                            tasks.len(),
                            mgr.running_count()
                        )];
                        for task in &tasks {
                            lines.push(format!(
                                "  {} [{}] {} ({:.1}s)",
                                task.task_id,
                                task.state,
                                task.description,
                                task.runtime_seconds()
                            ));
                        }
                        lines.join("\n")
                    }
                } else {
                    "Task manager busy. Try again.".to_string()
                };
                self.push_command_result(msg);
            }
            "task" => {
                self.push_slash_echo(cmd);
                match args {
                    Some(id) => {
                        let msg = if let Ok(mgr) = self.task_manager.try_lock() {
                            let output = mgr.read_output(id, 50);
                            if output.is_empty() {
                                format!("No output for task '{id}'")
                            } else {
                                format!("Output for task {id}:\n{output}")
                            }
                        } else {
                            "Task manager busy. Try again.".to_string()
                        };
                        self.push_command_result(msg);
                    }
                    None => {
                        self.push_command_result("Usage: /task <id>".to_string());
                    }
                }
            }
            "kill" => {
                self.push_slash_echo(cmd);
                match args {
                    Some(id) => {
                        let id = id.to_string();
                        let _ = self.event_tx.send(AppEvent::KillTask(id));
                    }
                    None => {
                        self.push_command_result("Usage: /kill <id>".to_string());
                    }
                }
            }
            "init" => {
                self.push_slash_echo(cmd);
                if self.state.agent_active {
                    self.push_command_result("Cannot run /init while agent is active".to_string());
                    return;
                }
                let prompt =
                    opendev_agents::prompts::embedded::build_init_prompt(args.unwrap_or(""));
                self.push_command_result("Generating AGENTS.md...".to_string());
                let _ = self.event_tx.send(AppEvent::UserSubmit(prompt));
            }
            "agents" => {
                self.push_slash_echo(cmd);
                match args {
                    Some("create") => {
                        self.push_command_result("Agent creation coming soon.".to_string());
                    }
                    _ => {
                        self.push_command_result("No custom agents configured".to_string());
                    }
                }
            }
            "skills" => {
                self.push_slash_echo(cmd);
                match args {
                    Some("create") => {
                        self.push_command_result("Skill creation coming soon.".to_string());
                    }
                    _ => {
                        self.push_command_result("No custom skills configured".to_string());
                    }
                }
            }
            "plugins" => {
                self.push_slash_echo(cmd);
                match args {
                    Some("install") => {
                        self.push_command_result("Plugin installation coming soon.".to_string());
                    }
                    Some("remove") => {
                        self.push_command_result("Plugin removal coming soon.".to_string());
                    }
                    _ => {
                        self.push_command_result("No plugins installed".to_string());
                    }
                }
            }
            "sound" => {
                self.push_slash_echo(cmd);
                opendev_runtime::play_finish_sound();
                self.push_command_result("Playing test sound...".to_string());
            }
            "compact" => {
                self.push_slash_echo(cmd);
                // Account for the echo we just pushed (< 6 means < 5 real messages)
                if self.state.messages.len() < 6 {
                    self.push_command_result(
                        "Not enough messages to compact (need at least 5)".to_string(),
                    );
                } else if self.state.compaction_active {
                    self.push_command_result("Compaction already in progress".to_string());
                } else if self.state.agent_active {
                    self.push_command_result("Cannot compact while agent is running".to_string());
                } else {
                    // Send special sentinel to trigger compaction in the backend
                    if let Some(ref tx) = self.user_message_tx {
                        let _ = tx.send("\x00__COMPACT__".to_string());
                    }
                }
            }
            "bg" => {
                self.push_slash_echo(cmd);
                match args {
                    None => {
                        // List all background agent tasks
                        let tasks = self.state.bg_agent_manager.all_tasks();
                        if tasks.is_empty() {
                            self.push_command_result("No background agents".to_string());
                        } else {
                            let mut lines = vec![format!(
                                "Background agents ({} total, {} running):",
                                tasks.len(),
                                self.state.bg_agent_manager.running_count()
                            )];
                            for task in &tasks {
                                let elapsed = task.runtime_seconds();
                                let elapsed_str = if elapsed >= 60.0 {
                                    format!("{}m {:.0}s", elapsed as u64 / 60, elapsed % 60.0)
                                } else {
                                    format!("{elapsed:.1}s")
                                };
                                let tool_info = if let Some(ref tool) = task.current_tool {
                                    format!(" ({})", tool)
                                } else {
                                    String::new()
                                };
                                let query_preview: String = task.query.chars().take(50).collect();
                                lines.push(format!(
                                    "  [{id}] [{state}] {query}{tool_info} — {elapsed_str}, {tools} tools",
                                    id = task.task_id,
                                    state = task.state,
                                    query = query_preview,
                                    tools = task.tool_call_count,
                                ));
                            }
                            self.push_command_result(lines.join("\n"));
                        }
                    }
                    Some(sub) if sub.starts_with("kill ") => {
                        let id = sub.strip_prefix("kill ").unwrap().trim();
                        if self.state.bg_agent_manager.kill_task(id) {
                            let _ = self.event_tx.send(AppEvent::BackgroundAgentKilled {
                                task_id: id.to_string(),
                            });
                        } else {
                            self.push_command_result(format!(
                                "Background agent '{id}' not found or not running"
                            ));
                        }
                    }
                    Some(sub) if sub.starts_with("merge ") => {
                        let id = sub.strip_prefix("merge ").unwrap().trim();
                        if let Some(task) = self.state.bg_agent_manager.get_task(id) {
                            if let Some(ref summary) = task.result_summary {
                                let content = format!("[Background agent {id} result]\n{summary}");
                                self.push_command_result(content);
                            } else {
                                self.push_command_result(format!(
                                    "Background agent '{id}' has no result yet"
                                ));
                            }
                        } else {
                            self.push_command_result(format!("Background agent '{id}' not found"));
                        }
                    }
                    Some(id) => {
                        // Show details for a specific task
                        if let Some(task) = self.state.bg_agent_manager.get_task(id) {
                            let elapsed = task.runtime_seconds();
                            let summary =
                                task.result_summary.as_deref().unwrap_or("(still running)");
                            self.push_command_result(format!(
                                "Background agent [{id}]:\n  Query: {}\n  State: {}\n  Tools: {}\n  Cost: ${:.4}\n  Elapsed: {:.1}s\n  Result: {summary}",
                                task.query, task.state, task.tool_call_count, task.cost_usd, elapsed
                            ));
                        } else {
                            self.push_command_result(format!("Background agent '{id}' not found"));
                        }
                    }
                }
            }
            "undo" => {
                self.push_slash_echo(cmd);
                if self.state.agent_active {
                    self.push_command_result("Cannot undo while agent is running".to_string());
                } else if let Some(ref tx) = self.user_message_tx {
                    let _ = tx.send("\x00__UNDO__".to_string());
                } else {
                    self.push_command_result("Undo not available".to_string());
                }
            }
            "redo" => {
                self.push_slash_echo(cmd);
                if self.state.agent_active {
                    self.push_command_result("Cannot redo while agent is running".to_string());
                } else if let Some(ref tx) = self.user_message_tx {
                    let _ = tx.send("\x00__REDO__".to_string());
                } else {
                    self.push_command_result("Redo not available".to_string());
                }
            }
            "share" => {
                self.push_slash_echo(cmd);
                if let Some(ref tx) = self.user_message_tx {
                    let _ = tx.send("\x00__SHARE__".to_string());
                } else {
                    self.push_command_result("Share not available".to_string());
                }
            }
            "sessions" => {
                self.push_slash_echo(cmd);
                // Open session picker
                if let Some(ref tx) = self.user_message_tx {
                    let _ = tx.send("\x00__LIST_SESSIONS__".to_string());
                }
                self.push_command_result("Loading sessions...".to_string());
            }
            "help" => {
                self.push_slash_echo(cmd);
                self.push_command_result(
                    [
                        "Available commands:",
                        "  /help              — Show this help",
                        "  /clear             — Clear conversation",
                        "  /mode [plan|normal]      — Toggle or set mode",
                        "  /autonomy [manual|semi-auto|auto] — Cycle or set autonomy",
                        "  /models              — Open model picker",
                        "  /session-models [name|clear] — Set model for session",
                        "  /sessions          — List saved sessions",
                        "  /undo              — Undo last file changes",
                        "  /redo              — Redo undone changes",
                        "  /share             — Share session as HTML",
                        "  /mcp [list|add|remove|enable|disable] — Manage MCP servers",
                        "  /tasks             — List background tasks",
                        "  /task <id>         — Show task output",
                        "  /kill <id>         — Kill a background task",
                        "  /init [path]       — Generate AGENTS.md",
                        "  /agents [list|create] — Manage custom agents",
                        "  /skills [list|create] — Manage custom skills",
                        "  /plugins [list|install|remove] — Manage plugins",
                        "  /sound             — Play test notification sound",
                        "  /compact           — Compact conversation context",
                        "  /bg                — List background agents",
                        "  /bg <id>           — Show background agent details",
                        "  /bg merge <id>     — Inject background agent result into conversation",
                        "  /bg kill <id>      — Kill a background agent",
                        "  /exit              — Quit OpenDev",
                        "",
                        "Keyboard shortcuts:",
                        "  Ctrl+C      — Clear input / interrupt / quit",
                        "  Ctrl+B      — Background running agent / toggle panel",
                        "  Ctrl+D      — Toggle debug panel",
                        "  Ctrl+R      — Open session picker",
                        "  Ctrl+X ...  — Leader key (u=undo, r=redo, s=share, m=models, d=debug)",
                        "  Alt+B       — Toggle task watcher panel",
                        "  Escape      — Interrupt agent",
                        "  Tab         — Accept autocomplete / toggle mode (when empty)",
                        "  Ctrl+I      — Toggle thinking block expand/collapse",
                        "  Shift+Tab   — Toggle mode",
                        "  PageUp/Down — Scroll conversation",
                    ]
                    .join("\n"),
                );
            }
            _ => {
                self.push_slash_echo(cmd);
                self.push_command_result(format!(
                    "Unknown command: /{name}. Type /help for available commands."
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "slash_commands_tests.rs"]
mod tests;
