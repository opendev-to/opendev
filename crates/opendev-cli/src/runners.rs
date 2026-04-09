//! Interactive and non-interactive session runners, plus event replay.

use tracing::info;

use crate::helpers::*;
use crate::runtime;

/// Create an EventStore wired to an EventBus for audit trail.
fn create_event_store(session_dir: &std::path::Path) -> opendev_history::event_store::EventStore {
    let event_bus = opendev_runtime::event_bus::EventBus::new();
    let bridge = opendev_runtime::event_bus::create_event_bus_bridge(event_bus);
    opendev_history::event_store::EventStore::new(session_dir.to_path_buf())
        .with_post_append(bridge)
}

/// Run non-interactive mode: execute a single prompt and exit.
pub async fn run_non_interactive(
    working_dir: &std::path::Path,
    prompt: &str,
    continue_session: bool,
    resume_id: Option<&str>,
    title: Option<&str>,
    agent: Option<&str>,
) {
    use opendev_history::{SessionListing, SessionManager};

    info!(prompt = %prompt, "Non-interactive mode");

    let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
    let session_dir = paths.project_sessions_dir(working_dir);
    let config = load_app_config(working_dir);

    // First-run detection: if no settings file exists, run setup wizard
    let mut config = if !crate::setup::config_exists() {
        println!("No configuration found. Starting first-time setup...");
        match crate::setup::run_setup_wizard().await {
            Ok(wizard_config) => wizard_config,
            Err(e) => {
                eprintln!("Setup cancelled: {e}");
                std::process::exit(0);
            }
        }
    } else {
        config
    };

    // Apply profile overrides (from OPENDEV_PROFILE env var)
    if let Ok(profile) = std::env::var("OPENDEV_PROFILE") {
        opendev_config::apply_profile(&mut config, &profile);
    }

    let mut session_manager = match SessionManager::new(session_dir.clone()) {
        Ok(sm) => sm.with_event_store(create_event_store(&session_dir)),
        Err(e) => {
            eprintln!("Failed to initialize session manager: {e}");
            std::process::exit(1);
        }
    };
    session_manager.write_project_marker(working_dir);

    // Handle session resume: --continue or --resume ID work in non-interactive mode too
    if continue_session {
        let listing = SessionListing::new(session_dir);
        match listing.find_latest_session() {
            Some(meta) => {
                info!(session_id = %meta.id, "Resuming most recent session (non-interactive)");
                if let Err(e) = session_manager.resume_session(&meta.id) {
                    eprintln!("Warning: failed to resume session {}: {e}", meta.id);
                    session_manager.create_session();
                }
            }
            None => {
                session_manager.create_session();
            }
        }
    } else if let Some(id) = resume_id {
        info!(session_id = %id, "Resuming session (non-interactive)");
        if let Err(e) = session_manager.resume_session(id) {
            eprintln!("Error: failed to resume session '{id}': {e}");
            std::process::exit(1);
        }
    } else {
        session_manager.create_session();
    }

    // Apply --title flag: set session title immediately
    if let Some(t) = title {
        session_manager.set_metadata("title", t);
    }

    // Apply --agent flag: set default agent override in config
    if let Some(a) = agent {
        config.default_agent = Some(a.to_string());
    }

    let mut agent_runtime = match runtime::AgentRuntime::new(config, working_dir, session_manager) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to initialize agent runtime: {e}");
            std::process::exit(1);
        }
    };

    // Connect MCP servers (best-effort, failures are logged)
    agent_runtime.start_mcp_connections();

    // Compose system prompt per-turn (resolves MCP instructions if available)
    agent_runtime.resolve_mcp_instructions().await;
    let system_prompt = agent_runtime.compose_system_prompt();

    match agent_runtime
        .run_query(prompt, &system_prompt, None, None, false)
        .await
    {
        Ok(result) => {
            println!("{}", result.content);
            if !result.success {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

/// Run the interactive TUI.
pub async fn run_interactive(
    working_dir: &std::path::Path,
    continue_session: bool,
    resume: Option<Option<String>>,
    initial_message: Option<String>,
    dangerously_skip_permissions: bool,
    theme_name: Option<String>,
    profile_name: Option<String>,
) {
    use opendev_history::{SessionListing, SessionManager};

    info!(
        working_dir = %working_dir.display(),
        continue_session,
        "Starting interactive TUI"
    );

    // Initialize session manager using project-scoped session directory
    let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
    let session_dir = paths.project_sessions_dir(working_dir);
    let config = load_app_config(working_dir);

    // First-run detection: if no settings file exists, run setup wizard
    let mut config = if !crate::setup::config_exists() {
        println!("No configuration found. Starting first-time setup...");
        match crate::setup::run_setup_wizard().await {
            Ok(wizard_config) => wizard_config,
            Err(e) => {
                eprintln!("Setup cancelled: {e}");
                std::process::exit(0);
            }
        }
    } else {
        config
    };

    // Apply profile overrides (from --profile flag or OPENDEV_PROFILE env var)
    let effective_profile = profile_name.or_else(|| std::env::var("OPENDEV_PROFILE").ok());
    if let Some(ref profile) = effective_profile {
        opendev_config::apply_profile(&mut config, profile);
    }

    let mut session_manager = match SessionManager::new(session_dir.clone()) {
        Ok(sm) => {
            sm.write_project_marker(working_dir);
            sm.with_event_store(create_event_store(&session_dir))
        }
        Err(e) => {
            eprintln!("Failed to initialize session manager: {}", e);
            std::process::exit(1);
        }
    };

    // Handle resume / continue
    if continue_session {
        let listing = SessionListing::new(session_dir.clone());
        match listing.find_latest_session() {
            Some(meta) => {
                info!(session_id = %meta.id, "Resuming most recent session");
                if let Err(e) = session_manager.resume_session(&meta.id) {
                    eprintln!("Failed to load session {}: {e}", meta.id);
                    session_manager.create_session();
                }
            }
            None => {
                session_manager.create_session();
            }
        }
    } else if let Some(resume_id) = resume {
        match resume_id {
            Some(id) => {
                info!(session_id = %id, "Resuming session");
                if let Err(e) = session_manager.resume_session(&id) {
                    eprintln!("Failed to load session '{id}': {e}");
                    std::process::exit(1);
                }
            }
            None => {
                // Interactive session picker — list across all projects
                let paths_for_listing = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
                let sessions =
                    SessionListing::list_all_sessions(&paths_for_listing.global_projects_dir());

                if sessions.is_empty() {
                    session_manager.create_session();
                } else {
                    println!("Available sessions:");
                    println!(
                        "  {:<3} {:<40} {:<12} {:<12} {:>4}",
                        "#", "Title", "ID", "Updated", "Msgs"
                    );
                    println!("  {}", "-".repeat(75));
                    for (i, meta) in sessions.iter().enumerate().take(20) {
                        let title = meta.title.as_deref().unwrap_or("(untitled)");
                        let display_title: String = if title.len() > 38 {
                            format!("{}...", &title[..title.floor_char_boundary(35)])
                        } else {
                            title.to_string()
                        };
                        let relative = format_relative_time(meta.updated_at);
                        let short_id = if meta.id.len() > 10 {
                            &meta.id[..10]
                        } else {
                            &meta.id
                        };
                        println!(
                            "  {:<3} {:<40} {:<12} {:<12} {:>4}",
                            i + 1,
                            display_title,
                            short_id,
                            relative,
                            meta.message_count,
                        );
                    }
                    println!();

                    use std::io::{self, Write};
                    loop {
                        print!("Enter session number (q to cancel, Enter for new): ");
                        let _ = io::stdout().flush();
                        let mut buf = String::new();
                        if io::stdin().read_line(&mut buf).is_ok() {
                            let input = buf.trim();
                            if input.is_empty() || input == "q" {
                                session_manager.create_session();
                                break;
                            } else if let Ok(n) = input.parse::<usize>() {
                                if n >= 1 && n <= sessions.len() {
                                    let selected = &sessions[n - 1];
                                    if let Err(e) = session_manager.resume_session(&selected.id) {
                                        eprintln!("Failed to load session: {e}");
                                        session_manager.create_session();
                                    }
                                    break;
                                } else {
                                    eprintln!("Invalid selection, try again.");
                                }
                            } else {
                                eprintln!("Invalid input, try again.");
                            }
                        } else {
                            session_manager.create_session();
                            break;
                        }
                    }
                }
            }
        }
    } else {
        session_manager.create_session();
    }

    let _ = dangerously_skip_permissions; // Will be wired to approval system

    // Create agent runtime (prompt composer is initialized inside)
    let mut agent_runtime =
        match runtime::AgentRuntime::new(config.clone(), working_dir, session_manager) {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to initialize agent runtime: {e}");
                std::process::exit(1);
            }
        };

    // Connect MCP servers (best-effort, failures are logged)
    agent_runtime.start_mcp_connections();

    // Resolve theme: CLI flag > auto-detect from terminal background
    let resolved_theme = theme_name
        .as_deref()
        .and_then(opendev_tui::ThemeName::from_str_loose)
        .unwrap_or_else(opendev_tui::auto_detect_theme);

    // Extract session ID for TUI display
    let session_id = agent_runtime
        .session_manager
        .current_session()
        .map(|s| s.id.clone());

    // Populate initial TUI state from config
    let wd_str = working_dir.display().to_string();
    let mut app_state = opendev_tui::AppState {
        model: config.model.clone(),
        path_shortener: opendev_tui::formatters::PathShortener::new(Some(&wd_str)),
        working_dir: wd_str,
        git_branch: detect_git_branch(working_dir),
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme: resolved_theme.theme(),
        theme_name: resolved_theme,
        reasoning_level: opendev_tui::app::ReasoningLevel::from_str_loose(&config.reasoning_effort),
        session_id,
        ..opendev_tui::AppState::default()
    };

    // Wire todo manager from runtime to TUI for panel sync
    app_state.todo_manager = Some(std::sync::Arc::clone(&agent_runtime.todo_manager));

    // Hydrate TUI with session history on resume/continue
    if let Some(session) = agent_runtime.session_manager.current_session() {
        for msg in &session.messages {
            match msg.role {
                opendev_models::Role::User => {
                    if msg.metadata.contains_key("display_hidden") {
                        continue;
                    }
                    // Skip system-injected messages (nudges, directives, internal)
                    if msg.metadata.contains_key("_msg_class") {
                        continue;
                    }
                    // Also skip system-injected messages from older sessions
                    if opendev_models::message::is_system_injected_content(&msg.content) {
                        continue;
                    }
                    app_state
                        .messages
                        .push(opendev_tui::app::DisplayMessage::new(
                            opendev_tui::app::DisplayRole::User,
                            msg.content.clone(),
                        ));
                }
                opendev_models::Role::Assistant => {
                    // Add reasoning/thinking trace if present
                    // reasoning_content = native model reasoning (o1/o3)
                    // thinking_trace = our internal thinking step
                    let trace = msg
                        .reasoning_content
                        .as_deref()
                        .or(msg.thinking_trace.as_deref())
                        .unwrap_or("");
                    if !trace.is_empty() {
                        app_state.messages.push(opendev_tui::app::DisplayMessage {
                            role: opendev_tui::app::DisplayRole::Reasoning,
                            content: trace.to_string(),
                            tool_call: None,
                            collapsed: true,
                            thinking_started_at: None,
                            thinking_duration_secs: Some(0),
                            thinking_finalized_at: None,
                        });
                    }
                    // Add assistant text
                    if !msg.content.is_empty() {
                        app_state
                            .messages
                            .push(opendev_tui::app::DisplayMessage::new(
                                opendev_tui::app::DisplayRole::Assistant,
                                msg.content.clone(),
                            ));
                    }
                    // Add tool calls (skip task_complete — it's an internal control tool)
                    for tc in &msg.tool_calls {
                        if tc.name == "task_complete" {
                            continue;
                        }
                        app_state.messages.push(opendev_tui::app::DisplayMessage {
                            role: opendev_tui::app::DisplayRole::Assistant,
                            content: String::new(),
                            tool_call: Some(opendev_tui::app::DisplayToolCall::from_model(tc)),
                            collapsed: false,
                            thinking_started_at: None,
                            thinking_duration_secs: None,
                            thinking_finalized_at: None,
                        });
                    }
                }
                opendev_models::Role::System => {} // Skip system messages
            }
        }
    }

    // Inject initial message as first user submission (handled by the agent task)
    if let Some(ref msg) = initial_message {
        app_state
            .messages
            .push(opendev_tui::app::DisplayMessage::new(
                opendev_tui::app::DisplayRole::User,
                msg.clone(),
            ));
    }

    // Telegram remote-control is only enabled via `opendev remote`, not in
    // normal TUI mode.  This avoids the remote session claim killing other
    // TUI instances that share the same bot token.
    let tui_runner =
        crate::tui_runner::TuiRunner::new(agent_runtime).with_initial_message(initial_message);

    match tui_runner.run(app_state).await {
        Ok(exit_info) => {
            print_exit_message(&exit_info);
        }
        Err(e) => {
            eprintln!("TUI error: {e}");
            std::process::exit(1);
        }
    }
}

/// Print a clean goodbye message after the TUI exits.
fn print_exit_message(exit_info: &opendev_tui::ExitInfo) {
    let Some(ref session_id) = exit_info.session_id else {
        return;
    };

    // Only show exit message if the session had activity
    if exit_info.message_count == 0 {
        return;
    }

    let cost_str = if exit_info.session_cost > 0.0 {
        if exit_info.session_cost < 0.01 {
            format!(" | Cost ${:.4}", exit_info.session_cost)
        } else {
            format!(" | Cost ${:.2}", exit_info.session_cost)
        }
    } else {
        String::new()
    };

    eprintln!();
    eprintln!("Session {session_id}{cost_str}");
    eprintln!("Resume this session: opendev -r {session_id}");
}

/// Replay recorded events from a JSONL file for debugging.
pub async fn run_replay(path: &std::path::Path) {
    use opendev_tui::event::load_recorded_events;

    if !path.exists() {
        eprintln!("Error: replay file not found: {}", path.display());
        std::process::exit(1);
    }

    match load_recorded_events(path) {
        Ok(events) => {
            println!("Replaying {} events from {}", events.len(), path.display());
            println!("{}", "-".repeat(60));
            for event in &events {
                let reconstructable = if event.to_app_event().is_some() {
                    ""
                } else {
                    " [not reconstructable]"
                };
                println!(
                    "[seq={:>5} t={:>8}ms] {}{}: {}",
                    event.seq,
                    event.timestamp_ms,
                    event.variant,
                    reconstructable,
                    serde_json::to_string(&event.payload).unwrap_or_default(),
                );
            }
            println!("{}", "-".repeat(60));
            println!("Replay complete: {} events", events.len());
        }
        Err(e) => {
            eprintln!("Error reading replay file: {e}");
            std::process::exit(1);
        }
    }
}
