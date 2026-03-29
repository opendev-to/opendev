//! Binary entry point for the OpenDev CLI.
//!
//! Parses command-line arguments with clap and dispatches to the
//! appropriate handler (interactive REPL, non-interactive prompt,
//! web UI, MCP management, etc.).

mod cli;
mod commands;
mod helpers;
mod runners;
mod runtime;
mod setup;
mod tui_runner;
mod web_executor;

use clap::Parser;
use tracing::info;

use cli::{Cli, Commands};
use helpers::{init_tracing, install_panic_handler};

#[tokio::main]
async fn main() {
    install_panic_handler();

    let cli = Cli::parse();

    // Determine if we'll be running in TUI mode (interactive without -p)
    let tui_mode = cli.prompt.is_none() && cli.command.is_none();
    init_tracing(cli.verbose, tui_mode);
    info!("OpenDev starting");

    // Resolve working directory
    let working_dir = cli
        .working_dir
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    if !working_dir.exists() {
        eprintln!(
            "Error: Working directory does not exist: {}",
            working_dir.display()
        );
        std::process::exit(1);
    }

    // Dispatch subcommands
    match cli.command {
        Some(Commands::Setup) => {
            commands::handle_setup().await;
        }
        Some(Commands::Config { action }) => {
            commands::handle_config(action, &working_dir).await;
        }
        Some(Commands::Mcp { action }) => {
            commands::handle_mcp(action, &working_dir);
        }
        Some(Commands::Run { action }) => {
            commands::handle_run(action, &working_dir).await;
        }
        Some(Commands::Session { action }) => {
            commands::handle_session(action, &working_dir);
        }
        None => {
            // Replay mode
            if let Some(ref replay_path) = cli.replay {
                runners::run_replay(replay_path).await;
                return;
            }

            // Interactive or non-interactive mode
            if let Some(prompt) = cli.prompt {
                runners::run_non_interactive(
                    &working_dir,
                    &prompt,
                    cli.continue_session,
                    cli.resume.as_ref().and_then(|r| r.as_deref()),
                    cli.title.as_deref(),
                    cli.agent.as_deref(),
                )
                .await;
            } else {
                runners::run_interactive(
                    &working_dir,
                    cli.continue_session,
                    cli.resume,
                    cli.message,
                    cli.dangerously_skip_permissions,
                    cli.theme,
                    cli.profile,
                )
                .await;
            }
        }
    }
}
