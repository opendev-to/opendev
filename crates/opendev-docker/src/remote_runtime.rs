//! Remote Docker runtime — interact with a Docker host via SSH + Docker CLI.
//!
//! Ports the Python `RemoteRuntime` class. Instead of HTTP to an in-container
//! server, we shell out to `docker -H ssh://user@host` or `ssh user@host docker ...`.

use tracing::{debug, info};

use crate::errors::{DockerError, Result};
use crate::models::{BashObservation, DockerConfig, IsAliveResponse};

/// Remote runtime that interacts with a Docker host over SSH.
#[derive(Debug)]
pub struct RemoteRuntime {
    /// SSH connection string, e.g. `user@host`.
    ssh_target: String,
    /// Optional SSH key path.
    ssh_key_path: Option<String>,
    /// Container ID on the remote host.
    container_id: Option<String>,
    /// Whether the runtime is closed.
    closed: bool,
}

impl RemoteRuntime {
    /// Create a new remote runtime.
    pub fn new(host: &str, user: Option<&str>, ssh_key_path: Option<String>) -> Self {
        let ssh_target = match user {
            Some(u) => format!("{u}@{host}"),
            None => host.to_string(),
        };
        Self {
            ssh_target,
            ssh_key_path,
            container_id: None,
            closed: false,
        }
    }

    /// Create from a `DockerConfig`.
    pub fn from_config(config: &DockerConfig) -> Result<Self> {
        let host = config.remote_host.as_deref().ok_or_else(|| {
            DockerError::Other("remote_host is required for RemoteRuntime".into())
        })?;
        Ok(Self::new(
            host,
            config.remote_user.as_deref(),
            config.ssh_key_path.clone(),
        ))
    }

    /// Set the container ID on the remote host.
    pub fn set_container_id(&mut self, id: impl Into<String>) {
        self.container_id = Some(id.into());
    }

    /// Run a docker command on the remote host via SSH.
    async fn run_remote_docker(
        &self,
        args: &[&str],
        timeout_secs: f64,
    ) -> Result<(String, String, i32)> {
        let mut cmd_args: Vec<String> = Vec::new();

        if let Some(ref key) = self.ssh_key_path {
            cmd_args.extend(["-i".into(), key.clone()]);
        }

        cmd_args.push(self.ssh_target.clone());
        cmd_args.push("docker".into());
        cmd_args.extend(args.iter().map(|s| s.to_string()));

        debug!("Running remote: ssh {}", cmd_args.join(" "));

        let output = tokio::time::timeout(
            std::time::Duration::from_secs_f64(timeout_secs),
            tokio::process::Command::new("ssh").args(&cmd_args).output(),
        )
        .await
        .map_err(|_| DockerError::Timeout {
            seconds: timeout_secs,
            operation: format!("ssh docker {}", args.join(" ")),
        })?
        .map_err(|e| DockerError::CommandFailed {
            message: format!("SSH command failed: {e}"),
            stderr: String::new(),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);

        Ok((stdout, stderr, code))
    }

    /// Health check — verify we can reach the remote Docker daemon.
    pub async fn is_alive(&self) -> IsAliveResponse {
        if self.closed {
            return IsAliveResponse {
                status: "error".into(),
                message: "Runtime is closed".into(),
            };
        }

        match self
            .run_remote_docker(&["info", "--format", "{{.ServerVersion}}"], 10.0)
            .await
        {
            Ok((_, _, 0)) => IsAliveResponse::default(),
            Ok((_, stderr, _)) => IsAliveResponse {
                status: "error".into(),
                message: stderr,
            },
            Err(e) => IsAliveResponse {
                status: "error".into(),
                message: e.to_string(),
            },
        }
    }

    /// Wait for the remote Docker daemon to become reachable.
    pub async fn wait_for_ready(&self, timeout: f64, poll_interval: f64) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs_f64() < timeout {
            let resp = self.is_alive().await;
            if resp.status == "ok" {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_secs_f64(poll_interval)).await;
        }
        false
    }

    /// Execute a command inside the remote container.
    pub async fn exec_in_container(
        &self,
        command: &str,
        timeout_secs: f64,
    ) -> Result<BashObservation> {
        let container_id = self
            .container_id
            .as_deref()
            .ok_or_else(|| DockerError::Other("No container ID set on remote runtime".into()))?;

        let (stdout, stderr, code) = self
            .run_remote_docker(&["exec", container_id, "bash", "-c", command], timeout_secs)
            .await?;

        let output = if stderr.is_empty() {
            stdout
        } else {
            format!("{stdout}{stderr}")
        };

        Ok(BashObservation {
            output,
            exit_code: Some(code),
            failure_reason: if code != 0 {
                Some(format!("Exit code {code}"))
            } else {
                None
            },
        })
    }

    /// Copy a file from host to the remote container.
    pub async fn copy_to_container(&self, local_path: &str, container_path: &str) -> Result<()> {
        let container_id = self
            .container_id
            .as_deref()
            .ok_or_else(|| DockerError::Other("No container ID set".into()))?;

        // First scp to remote host, then docker cp
        let remote_tmp = format!("/tmp/opendev_transfer_{}", uuid::Uuid::new_v4());

        // SCP to remote
        let mut scp_args: Vec<String> = Vec::new();
        if let Some(ref key) = self.ssh_key_path {
            scp_args.extend(["-i".into(), key.clone()]);
        }
        scp_args.push(local_path.to_string());
        scp_args.push(format!("{}:{}", self.ssh_target, remote_tmp));

        let output = tokio::process::Command::new("scp")
            .args(&scp_args)
            .output()
            .await
            .map_err(|e| DockerError::CommandFailed {
                message: format!("SCP failed: {e}"),
                stderr: String::new(),
            })?;

        if !output.status.success() {
            return Err(DockerError::CommandFailed {
                message: "SCP to remote host failed".into(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        // docker cp on remote
        self.run_remote_docker(
            &[
                "cp",
                &remote_tmp,
                &format!("{container_id}:{container_path}"),
            ],
            60.0,
        )
        .await?;

        // Clean up temp file
        let _ = self
            .run_remote_docker(&["exec", container_id, "rm", "-f", &remote_tmp], 10.0)
            .await;

        Ok(())
    }

    /// Copy a file from the remote container to host.
    pub async fn copy_from_container(&self, container_path: &str, local_path: &str) -> Result<()> {
        let container_id = self
            .container_id
            .as_deref()
            .ok_or_else(|| DockerError::Other("No container ID set".into()))?;

        let remote_tmp = format!("/tmp/opendev_transfer_{}", uuid::Uuid::new_v4());

        // docker cp on remote
        self.run_remote_docker(
            &[
                "cp",
                &format!("{container_id}:{container_path}"),
                &remote_tmp,
            ],
            60.0,
        )
        .await?;

        // SCP from remote
        let mut scp_args: Vec<String> = Vec::new();
        if let Some(ref key) = self.ssh_key_path {
            scp_args.extend(["-i".into(), key.clone()]);
        }
        scp_args.push(format!("{}:{}", self.ssh_target, remote_tmp));
        scp_args.push(local_path.to_string());

        let output = tokio::process::Command::new("scp")
            .args(&scp_args)
            .output()
            .await
            .map_err(|e| DockerError::CommandFailed {
                message: format!("SCP failed: {e}"),
                stderr: String::new(),
            })?;

        if !output.status.success() {
            return Err(DockerError::CommandFailed {
                message: "SCP from remote host failed".into(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    /// Close the runtime.
    pub async fn close(&mut self) {
        self.closed = true;
        info!("Remote runtime closed");
    }
}

#[cfg(test)]
#[path = "remote_runtime_tests.rs"]
mod tests;
