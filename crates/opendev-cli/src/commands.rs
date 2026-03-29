//! Subcommand handlers for setup, config, MCP, session, and run commands.

use std::collections::HashMap;
use std::path::PathBuf;

use opendev_mcp::config::load_config as load_mcp_config_file;
use tracing::info;

use crate::cli::*;
use crate::helpers::*;

/// Handle the top-level `opendev setup` command.
pub async fn handle_setup() {
    match crate::setup::run_setup_wizard().await {
        Ok(_config) => {
            info!("Setup wizard completed successfully");
        }
        Err(e) => {
            eprintln!("Setup failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Handle config subcommands.
pub async fn handle_config(action: ConfigAction, working_dir: &std::path::Path) {
    match action {
        ConfigAction::Setup => {
            println!("Running setup wizard...");
            println!("Tip: you can also run `opendev setup` directly.");
            match crate::setup::run_setup_wizard().await {
                Ok(_config) => {
                    info!("Setup wizard completed successfully");
                }
                Err(e) => {
                    eprintln!("Setup failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        ConfigAction::Show => {
            let config = load_app_config(working_dir);
            match serde_json::to_string_pretty(&config) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("Error: failed to serialize config: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Handle MCP subcommands.
pub fn handle_mcp(action: McpAction, working_dir: &std::path::Path) {
    match action {
        McpAction::List => {
            let config = load_mcp_config(working_dir);
            if config.mcp_servers.is_empty() {
                println!("No MCP servers configured.");
                println!("Add one with: opendev mcp add <name> <command> [args...]");
                return;
            }
            println!("MCP servers:");
            let mut names: Vec<&String> = config.mcp_servers.keys().collect();
            names.sort();
            for name in names {
                let server = &config.mcp_servers[name];
                let status = if server.enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                let auto = if server.auto_start {
                    "auto-start"
                } else {
                    "manual"
                };
                println!(
                    "  {name}  [{status}, {auto}]  {} {}",
                    server.command,
                    server.args.join(" ")
                );
            }
        }
        McpAction::Get { name } => {
            let config = load_mcp_config(working_dir);
            match config.mcp_servers.get(&name) {
                Some(server) => {
                    println!("MCP server: {name}");
                    println!("  Command   : {}", server.command);
                    println!("  Args      : {}", server.args.join(" "));
                    println!("  Transport : {}", server.transport);
                    println!("  Enabled   : {}", server.enabled);
                    println!("  Auto-start: {}", server.auto_start);
                    if let Some(url) = &server.url {
                        println!("  URL       : {url}");
                    }
                    if !server.env.is_empty() {
                        println!("  Environment:");
                        for (k, v) in &server.env {
                            // Mask values that look like secrets
                            let display_val =
                                if k.contains("KEY") || k.contains("SECRET") || k.contains("TOKEN")
                                {
                                    "***".to_string()
                                } else {
                                    v.clone()
                                };
                            println!("    {k}={display_val}");
                        }
                    }
                }
                None => {
                    eprintln!("Error: MCP server '{name}' not found.");
                    eprintln!("Run `opendev mcp list` to see configured servers.");
                    std::process::exit(1);
                }
            }
        }
        McpAction::Add {
            name,
            command,
            args,
            env,
            no_auto_start,
        } => {
            // Parse KEY=VALUE env pairs
            let mut env_map: HashMap<String, String> = HashMap::new();
            for pair in &env {
                if let Some((k, v)) = pair.split_once('=') {
                    env_map.insert(k.to_string(), v.to_string());
                } else {
                    eprintln!("Warning: ignoring invalid env format '{pair}' (expected KEY=VALUE)");
                }
            }

            let server_config = opendev_mcp::McpServerConfig {
                command: command.clone(),
                args: args.clone(),
                env: env_map,
                enabled: true,
                auto_start: !no_auto_start,
                ..Default::default()
            };

            // Load the global config, add the server, save back
            let paths = opendev_config::Paths::default();
            let global_mcp_path = paths.global_mcp_config();
            let mut mcp_config = load_mcp_config_file(&global_mcp_path).unwrap_or_default();
            mcp_config.mcp_servers.insert(name.clone(), server_config);
            save_global_mcp_config(&mcp_config);

            println!("Added MCP server '{name}': {command} {}", args.join(" "));
            if !env.is_empty() {
                println!("  Environment: {}", env.join(", "));
            }
            if no_auto_start {
                println!("  Auto-start: disabled");
            }
        }
        McpAction::Remove { name } => {
            let paths = opendev_config::Paths::default();
            let global_mcp_path = paths.global_mcp_config();
            let mut mcp_config = load_mcp_config_file(&global_mcp_path).unwrap_or_default();

            if mcp_config.mcp_servers.remove(&name).is_some() {
                save_global_mcp_config(&mcp_config);
                println!("Removed MCP server: {name}");
            } else {
                eprintln!("Error: MCP server '{name}' not found.");
                std::process::exit(1);
            }
        }
        McpAction::Enable { name } => {
            let paths = opendev_config::Paths::default();
            let global_mcp_path = paths.global_mcp_config();
            let mut mcp_config = load_mcp_config_file(&global_mcp_path).unwrap_or_default();

            match mcp_config.mcp_servers.get_mut(&name) {
                Some(server) => {
                    server.enabled = true;
                    save_global_mcp_config(&mcp_config);
                    println!("Enabled MCP server: {name}");
                }
                None => {
                    eprintln!("Error: MCP server '{name}' not found.");
                    std::process::exit(1);
                }
            }
        }
        McpAction::Disable { name } => {
            let paths = opendev_config::Paths::default();
            let global_mcp_path = paths.global_mcp_config();
            let mut mcp_config = load_mcp_config_file(&global_mcp_path).unwrap_or_default();

            match mcp_config.mcp_servers.get_mut(&name) {
                Some(server) => {
                    server.enabled = false;
                    save_global_mcp_config(&mcp_config);
                    println!("Disabled MCP server: {name}");
                }
                None => {
                    eprintln!("Error: MCP server '{name}' not found.");
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Handle session subcommands.
pub fn handle_session(action: SessionAction, working_dir: &std::path::Path) {
    let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
    let session_dir = paths.project_sessions_dir(working_dir);
    let session_manager = match opendev_history::SessionManager::new(session_dir) {
        Ok(sm) => sm,
        Err(e) => {
            eprintln!("Error: failed to initialize session manager: {e}");
            std::process::exit(1);
        }
    };

    match action {
        SessionAction::List {
            archived,
            max_count,
            json,
        } => {
            let sessions = session_manager.list_sessions(archived);
            let sessions: Vec<_> = sessions.into_iter().take(max_count).collect();

            if json {
                match serde_json::to_string_pretty(&sessions) {
                    Ok(output) => println!("{output}"),
                    Err(e) => {
                        eprintln!("Error: failed to serialize sessions: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }

            if sessions.is_empty() {
                println!("No sessions found.");
                if !archived {
                    println!("Tip: use --archived to include archived sessions.");
                }
                return;
            }

            let header_title = "TITLE";
            println!(
                "{:<38}  {:<20}  {:>5}  {header_title}",
                "ID", "UPDATED", "MSGS"
            );
            println!("{}", "-".repeat(90));
            for s in &sessions {
                let updated = s.updated_at.format("%Y-%m-%d %H:%M");
                let title = s.title.as_deref().unwrap_or("-");
                let title_display = if title.len() > 30 {
                    format!("{}...", &title[..27])
                } else {
                    title.to_string()
                };
                println!(
                    "{:<38}  {:<20}  {:>5}  {}",
                    s.id, updated, s.message_count, title_display
                );
            }
            println!(
                "\nShowing {} session(s). Use -n to show more.",
                sessions.len()
            );
        }
        SessionAction::Delete { id } => {
            if let Err(e) = session_manager.delete_session(&id) {
                eprintln!("Error: failed to delete session '{id}': {e}");
                std::process::exit(1);
            }
            println!("Deleted session: {id}");
        }
        SessionAction::Export { id } => {
            let session_id = if let Some(id) = id {
                id
            } else {
                // Use the most recent session
                let sessions = session_manager.list_sessions(false);
                match sessions.first() {
                    Some(s) => s.id.clone(),
                    None => {
                        eprintln!("Error: no sessions found.");
                        std::process::exit(1);
                    }
                }
            };

            match session_manager.load_session(&session_id) {
                Ok(session) => match serde_json::to_string_pretty(&session) {
                    Ok(output) => println!("{output}"),
                    Err(e) => {
                        eprintln!("Error: failed to serialize session: {e}");
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Error: failed to load session '{session_id}': {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Handle run subcommands.
pub async fn handle_run(action: RunAction, working_dir: &std::path::Path) {
    match action {
        RunAction::Ui { ui_port, ui_host } => {
            println!("Starting web UI on {}:{}...", ui_host, ui_port);

            let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
            let config = load_app_config(working_dir);

            // Initialize session manager for web server
            let session_dir = paths.project_sessions_dir(working_dir);
            let session_manager = match opendev_history::SessionManager::new(session_dir) {
                Ok(sm) => sm,
                Err(e) => {
                    eprintln!("Failed to initialize session manager: {e}");
                    std::process::exit(1);
                }
            };

            // Initialize user store
            let user_store = match opendev_http::UserStore::new(paths.global_dir()) {
                Ok(us) => us,
                Err(e) => {
                    eprintln!("Failed to initialize user store: {e}");
                    std::process::exit(1);
                }
            };

            let model_registry = opendev_config::ModelRegistry::new();

            let state = opendev_web::state::AppState::new(
                session_manager,
                config.clone(),
                working_dir.display().to_string(),
                user_store,
                model_registry,
            );

            // Initialize agent runtime for executing queries
            let runtime_session_dir = paths.project_sessions_dir(working_dir);
            let runtime_session_manager =
                match opendev_history::SessionManager::new(runtime_session_dir) {
                    Ok(sm) => sm,
                    Err(e) => {
                        eprintln!("Failed to initialize runtime session manager: {e}");
                        std::process::exit(1);
                    }
                };

            let system_prompt = crate::runtime::build_system_prompt(working_dir, &config);

            match crate::runtime::AgentRuntime::new(config, working_dir, runtime_session_manager) {
                Ok(mut agent_runtime) => {
                    agent_runtime.connect_mcp_servers().await;

                    // Take channel receivers and spawn bridge tasks for
                    // subagent events, approvals, ask-user, plan-approval
                    if let Some(receivers) = agent_runtime.channel_receivers.take() {
                        crate::web_executor::spawn_channel_bridges(receivers, state.clone());
                    }

                    let executor =
                        crate::web_executor::WebAgentExecutor::new(agent_runtime, system_prompt);
                    state
                        .set_agent_executor(std::sync::Arc::new(executor))
                        .await;
                    println!("Agent executor initialized — Web UI can execute queries");
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to initialize agent runtime: {e}\n\
                         Web UI will start but agent execution will be unavailable."
                    );
                }
            }

            // Serve static files from the Vite build output directory (if present)
            // Vite outputs to opendev/web/static/ relative to project root
            let static_dir =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../opendev/web/static");
            let static_path = if static_dir.exists() {
                Some(static_dir)
            } else {
                None
            };

            if let Err(e) =
                opendev_web::server::start_server(state, &ui_host, ui_port, static_path.as_deref())
                    .await
            {
                eprintln!("Web server error: {e}");
                std::process::exit(1);
            }
        }
    }
}
