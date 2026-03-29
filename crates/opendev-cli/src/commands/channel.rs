//! Channel subcommand handlers (Telegram add/remove/status/serve/pair/unpair).

use opendev_models::TelegramChannelConfig;

use crate::cli::*;
use crate::helpers::*;

/// Convert model DmPolicy to channel DmPolicy.
fn to_channel_dm_policy(p: &opendev_models::DmPolicy) -> opendev_channels::telegram::DmPolicy {
    match p {
        opendev_models::DmPolicy::Open => opendev_channels::telegram::DmPolicy::Open,
        opendev_models::DmPolicy::Pairing => opendev_channels::telegram::DmPolicy::Pairing,
        opendev_models::DmPolicy::Allowlist => opendev_channels::telegram::DmPolicy::Allowlist,
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
                run_telegram_serve(working_dir).await;
            } else {
                // Run in background — spawn a detached child process
                let exe = std::env::current_exe().expect("failed to get current executable path");
                let mut cmd = std::process::Command::new(exe);
                cmd.arg("channel").arg("serve").arg("--foreground");

                cmd.current_dir(working_dir);

                cmd.stdin(std::process::Stdio::null());
                cmd.stdout(std::process::Stdio::null());
                cmd.stderr(std::process::Stdio::null());

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

pub fn require_telegram(config: &opendev_models::AppConfig) -> TelegramChannelConfig {
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
    use tracing::info;

    let config = load_app_config(working_dir);

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
