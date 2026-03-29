//! Single-owner coordination for Telegram remote sessions.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

const REMOTE_SESSION_DIR: &str = "opendev-telegram-remote";
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Tracks ownership of a Telegram remote session for a specific bot token.
pub struct RemoteSessionClaim {
    pid_file: PathBuf,
    pid: u32,
}

impl RemoteSessionClaim {
    /// Claim ownership of the remote session for `bot_token`.
    ///
    /// If another process already owns the session, it is asked to terminate
    /// so the current process can take over polling.
    pub fn claim(bot_token: &str) -> io::Result<Self> {
        let base_dir = std::env::temp_dir().join(REMOTE_SESSION_DIR);
        Self::claim_in_dir(bot_token, &base_dir)
    }

    fn claim_in_dir(bot_token: &str, base_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(base_dir)?;

        let pid = std::process::id();
        let pid_file = base_dir.join(format!("{}.pid", token_fingerprint(bot_token)));

        if let Some(existing_pid) = read_pid(&pid_file)?
            && existing_pid != pid
        {
            info!("Telegram remote: terminating existing session owner pid={existing_pid}");
            terminate_process(existing_pid);

            let deadline = Instant::now() + SHUTDOWN_WAIT;
            while process_exists(existing_pid) && Instant::now() < deadline {
                std::thread::sleep(POLL_INTERVAL);
            }

            if process_exists(existing_pid) {
                warn!(
                    "Telegram remote: previous owner pid={} did not exit after SIGTERM; forcing shutdown",
                    existing_pid
                );
                force_kill_process(existing_pid);
            }
        }

        fs::write(&pid_file, pid.to_string())?;
        Ok(Self { pid_file, pid })
    }

    /// Returns true if this process still owns the remote session.
    pub fn is_current_owner(&self) -> bool {
        match read_pid(&self.pid_file) {
            Ok(Some(owner_pid)) => owner_pid == self.pid,
            Ok(None) => false,
            Err(err) => {
                debug!(
                    "Telegram remote: failed to read session owner file {}: {}",
                    self.pid_file.display(),
                    err
                );
                false
            }
        }
    }
}

impl Drop for RemoteSessionClaim {
    fn drop(&mut self) {
        if self.is_current_owner() {
            let _ = fs::remove_file(&self.pid_file);
        }
    }
}

fn token_fingerprint(bot_token: &str) -> String {
    let mut hasher = DefaultHasher::new();
    bot_token.hash(&mut hasher);
    format!("remote-{:016x}", hasher.finish())
}

fn read_pid(path: &Path) -> io::Result<Option<u32>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents.trim().parse::<u32>().ok()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // SAFETY: `kill(pid, 0)` does not send a signal and is the standard way to
    // check process existence on Unix.
    unsafe { libc::kill(pid as i32, 0) == 0 || errno_is_permission_denied() }
}

#[cfg(not(unix))]
fn process_exists(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn terminate_process(pid: u32) {
    // SAFETY: best-effort signal delivery to another local process.
    unsafe {
        let _ = libc::kill(pid as i32, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate_process(_pid: u32) {}

#[cfg(unix)]
fn force_kill_process(pid: u32) {
    // SAFETY: best-effort signal delivery to another local process.
    unsafe {
        let _ = libc::kill(pid as i32, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn force_kill_process(_pid: u32) {}

#[cfg(unix)]
fn errno_is_permission_denied() -> bool {
    io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_writes_current_pid_and_detects_owner_loss() {
        let dir = tempfile::tempdir().expect("temp dir");
        let claim = RemoteSessionClaim::claim_in_dir("token-a", dir.path()).expect("claim");

        assert!(claim.is_current_owner());

        fs::write(&claim.pid_file, "999999").expect("overwrite pid file");

        assert!(!claim.is_current_owner());
    }

    #[test]
    fn claim_overwrites_stale_owner_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pid_file = dir
            .path()
            .join(format!("{}.pid", token_fingerprint("token-b")));
        fs::write(&pid_file, "999999").expect("seed stale pid");

        let claim = RemoteSessionClaim::claim_in_dir("token-b", dir.path()).expect("claim");

        let owner = read_pid(&claim.pid_file).expect("read pid");
        assert_eq!(owner, Some(std::process::id()));
        assert!(claim.is_current_owner());
    }
}
