//! Subcommand handlers for setup, config, MCP, session, channel, and run commands.

use std::collections::HashMap;
use std::path::PathBuf;

use opendev_mcp::config::load_config as load_mcp_config_file;
use opendev_models::TelegramChannelConfig;
use tracing::info;

/// Convert model DmPolicy to channel DmPolicy.
fn to_channel_dm_policy(p: &opendev_models::DmPolicy) -> opendev_channels::telegram::DmPolicy {
    match p {
        opendev_models::DmPolicy::Open => opendev_channels::telegram::DmPolicy::Open,
        opendev_models::DmPolicy::Pairing => opendev_channels::telegram::DmPolicy::Pairing,
        opendev_models::DmPolicy::Allowlist => opendev_channels::telegram::DmPolicy::Allowlist,
    }
}

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

/// Handle channel subcommands.
pub async fn handle_channel(action: ChannelAction, working_dir: &std::path::Path) {
    match action {
        ChannelAction::Add { token } => {
            let bot_token = match token {
                Some(t) => t,
                None => {
                    eprint!("Enter Telegram bot token (from @BotFather): ");
                    let mut input = String::new();
                    std::io::stdin()
                        .read_line(&mut input)
                        .expect("failed to read input");
                    let trimmed = input.trim().to_string();
                    if trimmed.is_empty() {
                        eprintln!("Error: no token provided.");
                        std::process::exit(1);
                    }
                    trimmed
                }
            };

            eprint!("Validating...");
            let api = opendev_channels::telegram::api::TelegramApi::new(bot_token.clone());
            match api.get_me().await {
                Ok(user) => {
                    eprintln!(" @{}", user.username.as_deref().unwrap_or("unknown"));
                }
                Err(e) => {
                    eprintln!(" Failed: {e}");
                    std::process::exit(1);
                }
            }

            let paths = opendev_config::Paths::default();
            let global_settings = paths.global_settings();
            let mut config = load_app_config(working_dir);
            config.channels.telegram = Some(TelegramChannelConfig {
                bot_token,
                enabled: true,
                group_mention_only: true,
                dm_policy: opendev_models::DmPolicy::Pairing,
                allowed_users: Vec::new(),
            });

            save_config(&config, &global_settings);
            println!("Saved. Bot will auto-start on next launch.");
        }
        ChannelAction::Remove => {
            let paths = opendev_config::Paths::default();
            let global_settings = paths.global_settings();
            let mut config = load_app_config(working_dir);

            if config.channels.telegram.is_none() {
                eprintln!("Error: no channel configured.");
                std::process::exit(1);
            }

            config.channels.telegram = None;
            save_config(&config, &global_settings);
            println!("Removed telegram channel.");
        }
        ChannelAction::Status => {
            let config = load_app_config(working_dir);
            match &config.channels.telegram {
                Some(tg) if tg.enabled => {
                    let api =
                        opendev_channels::telegram::api::TelegramApi::new(tg.bot_token.clone());
                    match api.get_me().await {
                        Ok(user) => {
                            println!(
                                "telegram  @{}  {:?}",
                                user.username.as_deref().unwrap_or("unknown"),
                                tg.dm_policy,
                            );
                        }
                        Err(e) => println!("telegram  cannot connect ({e})"),
                    }
                    if tg.allowed_users.is_empty() {
                        println!("  no paired users");
                    } else {
                        for id in &tg.allowed_users {
                            println!("  paired: {id}");
                        }
                    }
                }
                Some(_) => println!("telegram  disabled"),
                None => {
                    println!("No channel configured. Run: opendev channel add");
                }
            }
        }
        ChannelAction::Serve { foreground } => {
            if foreground {
                // Run in foreground — block until Ctrl+C
                run_telegram_serve(working_dir).await;
            } else {
                // Run in background — spawn a detached child process
                let exe = std::env::current_exe().expect("failed to get current executable path");
                let mut cmd = std::process::Command::new(exe);
                cmd.arg("channel").arg("serve").arg("--foreground");

                // Pass through the working directory
                cmd.current_dir(working_dir);

                // Detach from the terminal
                cmd.stdin(std::process::Stdio::null());
                cmd.stdout(std::process::Stdio::null());
                cmd.stderr(std::process::Stdio::null());

                // Write PID file so we can stop it later
                let pid_path = opendev_config::Paths::default()
                    .global_dir()
                    .join("telegram-serve.pid");

                match cmd.spawn() {
                    Ok(child) => {
                        let pid = child.id();
                        if let Some(parent) = pid_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(&pid_path, pid.to_string());
                        println!("Telegram bot running in background (PID: {pid}).");
                        println!("Stop with: kill {pid}");
                    }
                    Err(e) => {
                        eprintln!("Error spawning background process: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        ChannelAction::Pair { user_id } => {
            let paths = opendev_config::Paths::default();
            let global_settings = paths.global_settings();
            let mut config = load_app_config(working_dir);

            {
                let tg = config
                    .channels
                    .telegram
                    .get_or_insert_with(|| TelegramChannelConfig {
                        bot_token: String::new(),
                        enabled: false,
                        group_mention_only: true,
                        dm_policy: opendev_models::DmPolicy::Pairing,
                        allowed_users: Vec::new(),
                    });

                if tg.allowed_users.contains(&user_id) {
                    println!("User {user_id} is already paired.");
                    return;
                }
                tg.allowed_users.push(user_id.clone());
            }

            let bot_token = config
                .channels
                .telegram
                .as_ref()
                .map(|t| t.bot_token.clone())
                .unwrap_or_default();

            save_config(&config, &global_settings);
            println!("Paired {user_id}.");

            // Notify user on Telegram
            if !bot_token.is_empty()
                && let Ok(chat_id) = user_id.parse::<i64>()
            {
                let api = opendev_channels::telegram::api::TelegramApi::new(bot_token);
                let _ = api
                    .send_message(opendev_channels::telegram::types::SendMessageRequest {
                        chat_id,
                        text: "Access approved. Send a message to start chatting.".to_string(),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
        }
        ChannelAction::Unpair { user_id } => {
            let paths = opendev_config::Paths::default();
            let global_settings = paths.global_settings();
            let mut config = load_app_config(working_dir);

            if let Some(ref mut tg) = config.channels.telegram {
                tg.allowed_users.retain(|id| id != &user_id);
                save_config(&config, &global_settings);
                println!("Unpaired {user_id}.");
            } else {
                eprintln!("Error: no channel configured.");
                std::process::exit(1);
            }
        }
    }
}

/// Run the Telegram bot in foreground (blocking until Ctrl+C).
async fn run_telegram_serve(working_dir: &std::path::Path) {
    let config = load_app_config(working_dir);
    let tg = require_telegram(&config);

    let system_prompt = crate::runtime::build_system_prompt(working_dir, &config);
    let router = std::sync::Arc::new(opendev_channels::MessageRouter::new());
    let executor = std::sync::Arc::new(crate::runtime::ChannelAgentExecutor::new(
        config.clone(),
        working_dir,
        system_prompt,
    ));
    router.set_executor(executor).await;

    let telegram_config = opendev_channels::telegram::TelegramConfig {
        bot_token: tg.bot_token.clone(),
        enabled: true,
        group_mention_only: tg.group_mention_only,
        dm_policy: to_channel_dm_policy(&tg.dm_policy),
        allowed_users: tg.allowed_users.clone(),
    };

    match opendev_channels::telegram::start_telegram(Some(&telegram_config), router).await {
        Ok((_adapter, _shutdown)) => {
            println!("Telegram bot running. Ctrl+C to stop.");
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for Ctrl+C");
            println!("\nShutting down...");
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn require_telegram(config: &opendev_models::AppConfig) -> TelegramChannelConfig {
    match &config.channels.telegram {
        Some(tg) if tg.enabled => tg.clone(),
        Some(_) => {
            eprintln!("Error: telegram channel is disabled.");
            std::process::exit(1);
        }
        None => {
            eprintln!("Error: no channel configured. Run: opendev channel add");
            std::process::exit(1);
        }
    }
}

/// Handle the `opendev remote` subcommand — headless Telegram remote session.
pub async fn handle_remote(
    working_dir: &std::path::Path,
    continue_session: bool,
    resume_id: Option<&str>,
) {
    use opendev_history::{SessionListing, SessionManager};

    let config = load_app_config(working_dir);

    // Require Telegram to be configured
    let tg = require_telegram(&config);

    let system_prompt = crate::runtime::build_system_prompt(working_dir, &config);
    let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
    let session_dir = paths.project_sessions_dir(working_dir);

    let mut session_manager = match SessionManager::new(session_dir.clone()) {
        Ok(sm) => sm,
        Err(e) => {
            eprintln!("Failed to initialize session manager: {e}");
            std::process::exit(1);
        }
    };

    // Handle session resume
    if continue_session {
        let listing = SessionListing::new(session_dir);
        match listing.find_latest_session() {
            Some(meta) => {
                info!("Resuming session: {}", meta.id);
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
        if let Err(e) = session_manager.resume_session(id) {
            eprintln!("Error: failed to resume session '{id}': {e}");
            std::process::exit(1);
        }
    } else {
        session_manager.create_session();
    }

    // Create agent runtime
    let agent_runtime =
        match crate::runtime::AgentRuntime::new(config.clone(), working_dir, session_manager) {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to initialize agent runtime: {e}");
                std::process::exit(1);
            }
        };

    // Start Telegram in remote-control mode
    let telegram_config = opendev_channels::telegram::TelegramConfig {
        bot_token: tg.bot_token.clone(),
        enabled: true,
        group_mention_only: false,
        dm_policy: to_channel_dm_policy(&tg.dm_policy),
        allowed_users: tg.allowed_users.clone(),
    };

    match opendev_channels::telegram::start_telegram_remote(Some(&telegram_config)).await {
        Ok((adapter, _shutdown, bridge, event_tx, command_rx)) => {
            let bot_name = adapter.bot_username();
            println!(
                "\n  OpenDev Remote Session\n\n  \
                 Bot:         @{}\n  \
                 Link:        https://t.me/{}\n  \
                 Working dir: {}\n  \
                 Model:       {}\n\n  \
                 Open the link above in Telegram to start.\n",
                bot_name,
                bot_name,
                working_dir.display(),
                config.model,
            );

            crate::remote_runner::run_remote(
                agent_runtime,
                system_prompt,
                event_tx,
                command_rx,
                bridge,
            )
            .await;
        }
        Err(e) => {
            eprintln!("Error starting Telegram remote: {e}");
            std::process::exit(1);
        }
    }
}

fn save_config(config: &opendev_models::AppConfig, path: &std::path::Path) {
    if let Err(e) = opendev_config::ConfigLoader::save(config, path) {
        eprintln!("Error: failed to save config: {e}");
        std::process::exit(1);
    }
}
