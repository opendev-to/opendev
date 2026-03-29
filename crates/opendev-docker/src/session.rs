//! Docker session — execute commands and copy files inside a container
//! using `docker exec` and `docker cp`.
//!
//! Ports the Python `BashSession` / `session.py`.

use regex::Regex;
use tracing::debug;

use crate::errors::{DockerError, Result};
use crate::models::{BashObservation, CheckMode};

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let re = Regex::new(r"\x1B\[[\d;]*[A-Za-z]|\x1B[@-_][0-?]*[ -/]*[@-~]").unwrap();
    re.replace_all(s, "").replace("\r\n", "\n")
}

/// A session representing a working context inside a Docker container.
///
/// Commands are executed via `docker exec`; files are transferred via `docker cp`.
pub struct DockerSession {
    container_id: String,
    name: String,
    working_dir: Option<String>,
}

impl DockerSession {
    /// Create a new session for the given container.
    pub fn new(container_id: &str, name: &str) -> Self {
        Self {
            container_id: container_id.to_string(),
            name: name.to_string(),
            working_dir: None,
        }
    }

    /// Set the working directory for commands in this session.
    pub fn set_working_dir(&mut self, dir: impl Into<String>) {
        self.working_dir = Some(dir.into());
    }

    /// Session name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Container ID this session is attached to.
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// Execute a command inside the container via `docker exec`.
    pub async fn exec_command(
        &self,
        command: &str,
        timeout_secs: f64,
        check: CheckMode,
    ) -> Result<BashObservation> {
        let mut args: Vec<String> = vec!["exec".into()];

        if let Some(ref wd) = self.working_dir {
            args.extend(["--workdir".into(), wd.clone()]);
        }

        args.push(self.container_id.clone());
        args.extend(["bash".into(), "-c".into(), command.into()]);

        debug!("docker exec in '{}': {}", self.name, command);

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = tokio::time::timeout(
            std::time::Duration::from_secs_f64(timeout_secs),
            tokio::process::Command::new("docker")
                .args(&arg_refs)
                .output(),
        )
        .await
        .map_err(|_| DockerError::Timeout {
            seconds: timeout_secs,
            operation: format!("docker exec: {command}"),
        })?
        .map_err(|e| DockerError::CommandFailed {
            message: format!("docker exec failed: {e}"),
            stderr: String::new(),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        let combined = strip_ansi(&if stderr.is_empty() {
            stdout
        } else if stdout.is_empty() {
            stderr
        } else {
            format!("{stdout}{stderr}")
        });

        let failure_reason = if exit_code != 0 {
            Some(format!("Exit code {exit_code}"))
        } else {
            None
        };

        if check == CheckMode::Raise && exit_code != 0 {
            return Err(DockerError::NonZeroExit {
                exit_code,
                command: command.to_string(),
                output: combined,
            });
        }

        Ok(BashObservation {
            output: combined.trim().to_string(),
            exit_code: if check == CheckMode::Ignore {
                None
            } else {
                Some(exit_code)
            },
            failure_reason,
        })
    }

    /// Copy a file from the host into the container.
    pub async fn copy_file_in(&self, host_path: &str, container_path: &str) -> Result<()> {
        let dest = format!("{}:{}", self.container_id, container_path);
        let output = tokio::process::Command::new("docker")
            .args(["cp", host_path, &dest])
            .output()
            .await
            .map_err(|e| DockerError::CommandFailed {
                message: format!("docker cp failed: {e}"),
                stderr: String::new(),
            })?;

        if !output.status.success() {
            return Err(DockerError::CommandFailed {
                message: "docker cp into container failed".into(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    /// Copy a file from the container to the host.
    pub async fn copy_file_out(&self, container_path: &str, host_path: &str) -> Result<()> {
        let src = format!("{}:{}", self.container_id, container_path);
        let output = tokio::process::Command::new("docker")
            .args(["cp", &src, host_path])
            .output()
            .await
            .map_err(|e| DockerError::CommandFailed {
                message: format!("docker cp failed: {e}"),
                stderr: String::new(),
            })?;

        if !output.status.success() {
            return Err(DockerError::CommandFailed {
                message: "docker cp from container failed".into(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    /// Send an interrupt (kill -INT) to all processes in the container.
    pub async fn interrupt(&self) -> Result<String> {
        let obs = self
            .exec_command("kill -INT -1 2>/dev/null; true", 5.0, CheckMode::Ignore)
            .await?;
        Ok(obs.output)
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
