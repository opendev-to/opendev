//! Configuration loading, tracing setup, crash handler, and utility functions.

use opendev_mcp::config::{
    get_project_config_path, load_config as load_mcp_config_file, merge_configs,
    save_config as save_mcp_config_file,
};

pub fn init_tracing(verbose: bool, tui_mode: bool) {
    use tracing_subscriber::EnvFilter;

    let filter = if verbose {
        EnvFilter::new("debug")
    } else if tui_mode {
        EnvFilter::new("warn")
    } else {
        EnvFilter::new("info")
    };

    if tui_mode {
        // Redirect logs to file so they don't corrupt the alternate screen
        if let Some(home) = dirs_next::home_dir() {
            let log_dir = home.join(".opendev").join("logs");
            let _ = std::fs::create_dir_all(&log_dir);
            if let Ok(file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_dir.join("opendev.log"))
            {
                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_target(false)
                    .with_ansi(false)
                    .with_writer(file)
                    .init();
                return;
            }
        }
        // Fallback: suppress everything if we can't open log file
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("error"))
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
    }
}

/// Load the merged AppConfig using standard paths for the given working directory.
pub fn load_app_config(working_dir: &std::path::Path) -> opendev_models::AppConfig {
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
pub fn load_mcp_config(working_dir: &std::path::Path) -> opendev_mcp::McpConfig {
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
pub fn save_global_mcp_config(config: &opendev_mcp::McpConfig) {
    let paths = opendev_config::Paths::default();
    let global_mcp_path = paths.global_mcp_config();
    if let Err(e) = save_mcp_config_file(config, &global_mcp_path) {
        eprintln!("Error: failed to save MCP config: {e}");
        std::process::exit(1);
    }
}

/// Install a custom panic hook that writes crash reports to `~/.opendev/crash/`.
pub fn install_panic_handler() {
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

/// Format a timestamp as a relative time string (e.g., "just now", "5m ago").
pub fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);
    let secs = diff.num_seconds();

    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Detect the current git branch for the given directory.
pub fn detect_git_branch(working_dir: &std::path::Path) -> Option<String> {
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
