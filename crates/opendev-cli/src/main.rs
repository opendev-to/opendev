//! Binary entry point for the OpenDev CLI.
//!
//! Mirrors `opendev/cli/main.py`.
//!
//! Parses command-line arguments with clap and dispatches to the
//! appropriate handler (interactive REPL, non-interactive prompt,
//! web UI, MCP management, etc.).

mod runtime;
mod setup;
mod tui_runner;

use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use opendev_mcp::config::{
    get_project_config_path, load_config as load_mcp_config_file, merge_configs,
    save_config as save_mcp_config_file,
};
use tracing::info;

/// OpenDev — AI-powered command-line tool for accelerated development.
#[derive(Parser, Debug)]
#[command(
    name = "opendev",
    version,
    about = "OpenDev — AI-powered command-line tool for accelerated development",
    long_about = None,
    after_help = "Examples:\n  \
        opendev                          Start interactive CLI session\n  \
        opendev \"do something\"           Start session with initial message\n  \
        opendev -p \"create hello.py\"     Non-interactive mode\n  \
        opendev --continue               Resume most recent session\n  \
        opendev run ui                   Start web UI\n  \
        opendev mcp list                 List MCP servers"
)]
struct Cli {
    /// Execute a single prompt and exit (non-interactive mode).
    #[arg(short, long, value_name = "TEXT")]
    prompt: Option<String>,

    /// Set working directory (defaults to current directory).
    #[arg(short = 'd', long = "working-dir", value_name = "PATH")]
    working_dir: Option<PathBuf>,

    /// Enable verbose output with detailed logging.
    #[arg(short, long)]
    verbose: bool,

    /// Resume the most recent session for the current working directory.
    #[arg(short = 'c', long = "continue")]
    continue_session: bool,

    /// Resume a session (optionally specify ID, or pick interactively).
    #[arg(short = 'r', long, value_name = "SESSION_ID")]
    resume: Option<Option<String>>,

    /// Skip all permission prompts and auto-approve every operation.
    #[arg(long)]
    dangerously_skip_permissions: bool,

    /// Initial message to start the session with (positional).
    #[arg(value_name = "MESSAGE")]
    message: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

/// Top-level subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the interactive setup wizard (first-run or re-configure).
    Setup,

    /// Manage OpenDev configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Configure and manage MCP servers.
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Run development tools.
    Run {
        #[command(subcommand)]
        action: RunAction,
    },
}

/// Config subcommands.
#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Run the interactive setup wizard.
    Setup,
    /// Display current configuration.
    Show,
}

/// MCP subcommands.
#[derive(Subcommand, Debug)]
enum McpAction {
    /// List all configured MCP servers.
    List,
    /// Show detailed information about a specific server.
    Get {
        /// Server name.
        name: String,
    },
    /// Add a new MCP server.
    Add {
        /// Unique name for the server.
        name: String,
        /// Command to start the server (e.g., "uvx", "node", "python").
        command: String,
        /// Arguments to pass to the command.
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
        /// Environment variables (KEY=VALUE).
        #[arg(long, value_name = "KEY=VALUE")]
        env: Vec<String>,
        /// Don't auto-start this server on launch.
        #[arg(long)]
        no_auto_start: bool,
    },
    /// Remove an MCP server.
    Remove {
        /// Server name.
        name: String,
    },
    /// Enable an MCP server.
    Enable {
        /// Server name.
        name: String,
    },
    /// Disable an MCP server.
    Disable {
        /// Server name.
        name: String,
    },
}

/// Run subcommands.
#[derive(Subcommand, Debug)]
enum RunAction {
    /// Start the web UI (backend + frontend).
    Ui {
        /// Port for backend API server.
        #[arg(long, default_value_t = 8080)]
        ui_port: u16,
        /// Host for backend API server.
        #[arg(long, default_value = "127.0.0.1")]
        ui_host: String,
    },
}

fn init_tracing(verbose: bool, tui_mode: bool) {
    use tracing_subscriber::EnvFilter;

    let filter = if verbose {
        EnvFilter::new("debug")
    } else if tui_mode {
        // Suppress logs in TUI mode — they conflict with the alternate screen
        EnvFilter::new("warn")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(if tui_mode {
            // Write to stderr in TUI mode so logs don't corrupt the alternate screen.
            // At warn level this should produce almost no output.
            std::io::stderr as fn() -> std::io::Stderr
        } else {
            std::io::stderr as fn() -> std::io::Stderr
        })
        .init();
}

/// Load the merged AppConfig using standard paths for the given working directory.
fn load_app_config(working_dir: &std::path::Path) -> opendev_models::AppConfig {
    let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
    let global_settings = paths.global_settings();
    let project_settings = paths.project_settings();

    match opendev_config::ConfigLoader::load(&global_settings, &project_settings) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Warning: failed to load config: {e}");
            opendev_models::AppConfig::default()
        }
    }
}

/// Load the merged MCP configuration (global + project).
fn load_mcp_config(working_dir: &std::path::Path) -> opendev_mcp::McpConfig {
    let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
    let global_mcp_path = paths.global_mcp_config();
    let project_mcp_path = get_project_config_path(working_dir);

    let global_config = match load_mcp_config_file(&global_mcp_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: failed to load global MCP config: {e}");
            opendev_mcp::McpConfig::default()
        }
    };

    let project_config = project_mcp_path
        .as_deref()
        .and_then(|p| load_mcp_config_file(p).ok());

    merge_configs(&global_config, project_config.as_ref())
}

/// Save the global MCP configuration.
fn save_global_mcp_config(config: &opendev_mcp::McpConfig) {
    let paths = opendev_config::Paths::default();
    let global_mcp_path = paths.global_mcp_config();
    if let Err(e) = save_mcp_config_file(config, &global_mcp_path) {
        eprintln!("Error: failed to save MCP config: {e}");
        std::process::exit(1);
    }
}

/// Install a custom panic hook that writes crash reports to `~/.opendev/crash/`.
fn install_panic_handler() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Capture backtrace
        let backtrace = std::backtrace::Backtrace::force_capture();

        // Build crash report
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let mut report = String::new();
        report.push_str("OpenDev Crash Report\n");
        report.push_str(&format!("Timestamp: {}\n", chrono::Utc::now()));
        report.push_str(&format!("Version: {}\n\n", env!("CARGO_PKG_VERSION")));

        // Panic message
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic payload".to_string()
        };
        report.push_str(&format!("Panic: {}\n", message));

        if let Some(location) = panic_info.location() {
            report.push_str(&format!(
                "Location: {}:{}:{}\n",
                location.file(),
                location.line(),
                location.column()
            ));
        }

        report.push_str(&format!("\nBacktrace:\n{}\n", backtrace));

        // Write crash report to ~/.opendev/crash/
        if let Some(home) = dirs_next::home_dir() {
            let crash_dir = home.join(".opendev").join("crash");
            if let Ok(()) = std::fs::create_dir_all(&crash_dir) {
                let filename = format!("crash-{}.log", timestamp);
                let crash_path = crash_dir.join(&filename);
                if std::fs::write(&crash_path, &report).is_ok() {
                    eprintln!(
                        "\nOpenDev crashed unexpectedly. A crash report has been saved to:\n  {}\n\nPlease include this file when reporting the issue.\n",
                        crash_path.display()
                    );
                } else {
                    eprintln!("\nOpenDev crashed unexpectedly. Failed to write crash report.\n");
                }
            }
        }

        // Call the default panic handler
        default_hook(panic_info);
    }));
}

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
            handle_setup().await;
        }
        Some(Commands::Config { action }) => {
            handle_config(action, &working_dir).await;
        }
        Some(Commands::Mcp { action }) => {
            handle_mcp(action, &working_dir);
        }
        Some(Commands::Run { action }) => {
            handle_run(action, &working_dir).await;
        }
        None => {
            // Interactive or non-interactive mode
            if let Some(prompt) = cli.prompt {
                run_non_interactive(&working_dir, &prompt).await;
            } else {
                run_interactive(
                    &working_dir,
                    cli.continue_session,
                    cli.resume,
                    cli.message,
                    cli.dangerously_skip_permissions,
                )
                .await;
            }
        }
    }
}

/// Handle the top-level `opendev setup` command.
async fn handle_setup() {
    match setup::run_setup_wizard().await {
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
async fn handle_config(action: ConfigAction, working_dir: &std::path::Path) {
    match action {
        ConfigAction::Setup => {
            println!("Running setup wizard...");
            println!("Tip: you can also run `opendev setup` directly.");
            match setup::run_setup_wizard().await {
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
fn handle_mcp(action: McpAction, working_dir: &std::path::Path) {
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

/// Handle run subcommands.
async fn handle_run(action: RunAction, working_dir: &std::path::Path) {
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
                config,
                working_dir.display().to_string(),
                user_store,
                model_registry,
            );

            // Serve static files from the bundled web-ui build directory (if present)
            let static_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web-ui/dist");
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

/// Run non-interactive mode: execute a single prompt and exit.
async fn run_non_interactive(working_dir: &std::path::Path, prompt: &str) {
    use opendev_history::SessionManager;

    info!(prompt = %prompt, "Non-interactive mode");

    let paths = opendev_config::Paths::new(Some(working_dir.to_path_buf()));
    let session_dir = paths.project_sessions_dir(working_dir);
    let config = load_app_config(working_dir);

    // First-run detection: if no settings file exists, run setup wizard
    let config = if !setup::config_exists() {
        println!("No configuration found. Starting first-time setup...");
        match setup::run_setup_wizard().await {
            Ok(wizard_config) => wizard_config,
            Err(e) => {
                eprintln!("Setup cancelled: {e}");
                std::process::exit(0);
            }
        }
    } else {
        config
    };

    // Build system prompt before config is moved
    let system_prompt = runtime::build_system_prompt(working_dir, &config);

    let mut session_manager = match SessionManager::new(session_dir) {
        Ok(sm) => sm,
        Err(e) => {
            eprintln!("Failed to initialize session manager: {e}");
            std::process::exit(1);
        }
    };

    // Create a fresh session for this one-shot query
    session_manager.create_session();

    let mut agent_runtime = match runtime::AgentRuntime::new(config, working_dir, session_manager) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to initialize agent runtime: {e}");
            std::process::exit(1);
        }
    };

    match agent_runtime.run_query(prompt, &system_prompt, None).await {
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
async fn run_interactive(
    working_dir: &std::path::Path,
    continue_session: bool,
    resume: Option<Option<String>>,
    initial_message: Option<String>,
    dangerously_skip_permissions: bool,
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
    let config = if !setup::config_exists() {
        println!("No configuration found. Starting first-time setup...");
        match setup::run_setup_wizard().await {
            Ok(wizard_config) => wizard_config,
            Err(e) => {
                eprintln!("Setup cancelled: {e}");
                std::process::exit(0);
            }
        }
    } else {
        config
    };

    let mut session_manager = match SessionManager::new(session_dir.clone()) {
        Ok(sm) => sm,
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
                // Interactive session picker (printed before entering TUI)
                let listing = SessionListing::new(session_dir.clone());
                let sessions = listing.list_sessions(None, false);

                if sessions.is_empty() {
                    session_manager.create_session();
                } else {
                    println!("Available sessions:");
                    for (i, meta) in sessions.iter().enumerate().take(20) {
                        let title = meta.title.as_deref().unwrap_or("(untitled)");
                        println!(
                            "  {}. {} — {} ({} messages, {})",
                            i + 1,
                            meta.id,
                            title,
                            meta.message_count,
                            meta.updated_at.format("%Y-%m-%d %H:%M"),
                        );
                    }
                    println!();

                    use std::io::{self, Write};
                    print!("Enter session number (or press Enter for new): ");
                    let _ = io::stdout().flush();
                    let mut buf = String::new();
                    if io::stdin().read_line(&mut buf).is_ok() {
                        let input = buf.trim();
                        if input.is_empty() {
                            session_manager.create_session();
                        } else if let Ok(n) = input.parse::<usize>() {
                            if n >= 1 && n <= sessions.len() {
                                let selected = &sessions[n - 1];
                                if let Err(e) = session_manager.resume_session(&selected.id) {
                                    eprintln!("Failed to load session: {e}");
                                    session_manager.create_session();
                                }
                            } else {
                                eprintln!("Invalid selection. Starting a new session.");
                                session_manager.create_session();
                            }
                        } else {
                            eprintln!("Invalid input. Starting a new session.");
                            session_manager.create_session();
                        }
                    } else {
                        session_manager.create_session();
                    }
                }
            }
        }
    } else {
        session_manager.create_session();
    }

    let _ = dangerously_skip_permissions; // Will be wired to approval system

    // Build system prompt from embedded templates
    let system_prompt = runtime::build_system_prompt(working_dir, &config);

    // Create agent runtime
    let agent_runtime =
        match runtime::AgentRuntime::new(config.clone(), working_dir, session_manager) {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to initialize agent runtime: {e}");
                std::process::exit(1);
            }
        };

    // Populate initial TUI state from config
    let mut app_state = opendev_tui::AppState {
        model: config.model.clone(),
        working_dir: shorten_working_dir(working_dir),
        git_branch: detect_git_branch(working_dir),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ..opendev_tui::AppState::default()
    };

    // Inject initial message as first user submission (handled by the agent task)
    if let Some(ref msg) = initial_message {
        app_state.messages.push(opendev_tui::app::DisplayMessage {
            role: opendev_tui::app::DisplayRole::User,
            content: msg.clone(),
            tool_call: None,
        });
    }

    // Create and run the TUI runner
    let tui_runner = tui_runner::TuiRunner::new(agent_runtime, system_prompt)
        .with_initial_message(initial_message);

    if let Err(e) = tui_runner.run(app_state).await {
        eprintln!("TUI error: {e}");
        std::process::exit(1);
    }
}

/// Shorten a working directory path for display.
fn shorten_working_dir(path: &std::path::Path) -> String {
    if let Some(home) = dirs_next::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// Detect the current git branch for the given directory.
fn detect_git_branch(working_dir: &std::path::Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(working_dir)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}
