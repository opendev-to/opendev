//! Microsandbox runtime detection, auto-start, and lifecycle management.
//!
//! Handles finding the `msb` binary (bundled or system-installed),
//! starting the server automatically, and health-checking readiness.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::errors::{Result, SandboxError};

/// Well-known location for the bundled microsandbox runtime.
const BUNDLED_MSB_DIR: &str = ".opendev/runtime/msb";

/// Default microsandbox server address.
const MSB_SERVER_URL: &str = "http://127.0.0.1:5555";

/// Resolve a path under the Homebrew-installed microsandbox libexec.
#[cfg(target_os = "macos")]
fn find_brew_msb_path(subpath: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("brew")
        .args(["--prefix", "opendev"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = PathBuf::from(&prefix).join("libexec/msb").join(subpath);
    path.exists().then_some(path)
}

/// Find the microsandbox `msb` binary.
///
/// Search order:
/// 1. Bundled with OpenDev at `~/.opendev/runtime/msb/bin/msb`
/// 2. Homebrew libexec (macOS): `$(brew --prefix opendev)/libexec/msb/bin/msb`
/// 3. System PATH
pub fn find_msb_binary() -> Option<PathBuf> {
    // 1. Bundled location
    if let Some(home) = dirs_next::home_dir() {
        let bundled = home.join(BUNDLED_MSB_DIR).join("bin/msb");
        if bundled.exists() {
            debug!(path = %bundled.display(), "Found bundled msb binary");
            return Some(bundled);
        }
    }

    // 2. Homebrew libexec (macOS)
    #[cfg(target_os = "macos")]
    if let Some(path) = find_brew_msb_path("bin/msb") {
        debug!(path = %path.display(), "Found Homebrew msb binary");
        return Some(path);
    }

    // 3. System PATH
    if let Ok(path) = which::which("msb") {
        debug!(path = %path.display(), "Found msb in PATH");
        return Some(path);
    }

    None
}

/// Find the library directory containing `libkrunfw`.
///
/// Search order matches `find_msb_binary`: bundled → Homebrew → system.
pub fn find_msb_lib_dir() -> Option<PathBuf> {
    if let Some(home) = dirs_next::home_dir() {
        let bundled = home.join(BUNDLED_MSB_DIR).join("lib");
        if bundled.exists() {
            return Some(bundled);
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(path) = find_brew_msb_path("lib") {
        return Some(path);
    }

    None
}

/// Check if the microsandbox server is reachable.
async fn health_check() -> bool {
    let url = format!("{MSB_SERVER_URL}/health");
    match reqwest::get(&url).await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Ensure the microsandbox server is running, starting it if necessary.
///
/// 1. Check if server is already running (health check)
/// 2. Find the msb binary (bundled or system)
/// 3. Start the server as a background process
/// 4. Wait for readiness
pub async fn ensure_server_running() -> Result<()> {
    // Already running?
    if health_check().await {
        debug!("Microsandbox server already running");
        return Ok(());
    }

    // Find the binary
    let msb = find_msb_binary().ok_or_else(|| {
        SandboxError::ServerUnavailable(
            "msb binary not found. Reinstall OpenDev or run: \
             curl -sSL https://get.microsandbox.dev | sh"
                .to_string(),
        )
    })?;

    info!(binary = %msb.display(), "Starting microsandbox server");

    // Build command with library path
    let mut cmd = tokio::process::Command::new(&msb);
    cmd.args(["server", "start", "--dev"]);

    // Set library path so libkrunfw is found
    if let Some(lib_dir) = find_msb_lib_dir() {
        #[cfg(target_os = "macos")]
        cmd.env("DYLD_LIBRARY_PATH", &lib_dir);

        #[cfg(target_os = "linux")]
        cmd.env("LD_LIBRARY_PATH", &lib_dir);
    }

    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    cmd.spawn()
        .map_err(|e| SandboxError::ServerUnavailable(format!("Failed to start msb server: {e}")))?;

    // Wait for readiness (poll health endpoint)
    for attempt in 0..30 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if health_check().await {
            info!(attempts = attempt + 1, "Microsandbox server ready");
            return Ok(());
        }
    }

    Err(SandboxError::ServerUnavailable(
        "Microsandbox server failed to become ready within 6 seconds".to_string(),
    ))
}

/// Check whether sandbox execution is available on this platform.
///
/// Returns `None` if available, or `Some(reason)` if not.
pub fn platform_availability() -> Option<String> {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return Some(
            "Sandbox execution requires Apple Silicon (M1+). Not available on Intel Mac."
                .to_string(),
        );
    }

    #[cfg(target_os = "windows")]
    {
        return Some("Sandbox execution is not yet available on Windows.".to_string());
    }

    #[allow(unreachable_code)]
    None
}

/// Stop the microsandbox server if it was started by us.
pub async fn stop_server() {
    if let Some(msb) = find_msb_binary() {
        let result = tokio::process::Command::new(&msb)
            .args(["server", "stop"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        match result {
            Ok(status) if status.success() => {
                info!("Microsandbox server stopped");
            }
            Ok(status) => {
                warn!(exit_code = ?status.code(), "msb server stop returned non-zero");
            }
            Err(e) => {
                warn!(error = %e, "Failed to stop microsandbox server");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_availability_current() {
        // On Apple Silicon macOS (our dev machine), should be available.
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert!(platform_availability().is_none());

        // On Intel Mac, should NOT be available.
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        assert!(platform_availability().is_some());
    }

    #[test]
    fn test_find_msb_binary_returns_option() {
        // Just verify it doesn't panic — result depends on system.
        let _ = find_msb_binary();
    }

    #[test]
    fn test_find_msb_lib_dir_returns_option() {
        let _ = find_msb_lib_dir();
    }
}
