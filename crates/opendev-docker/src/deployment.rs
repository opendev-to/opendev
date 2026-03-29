//! Docker container lifecycle management.
//!
//! Ports the Python `DockerDeployment` class — pull images, create/start/stop
//! containers, and inspect their status.

use std::collections::HashMap;
use std::net::TcpListener;

use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::errors::{DockerError, Result};
use crate::models::{ContainerStatus, DockerConfig};

/// Find a free TCP port on the host.
pub fn find_free_port() -> Result<u16> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| DockerError::Other(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| DockerError::Other(e.to_string()))?
        .port();
    Ok(port)
}

/// Run a Docker CLI command and return (stdout, stderr, exit_code).
pub async fn run_docker_command(
    args: &[&str],
    timeout_secs: f64,
    check: bool,
) -> Result<(String, String, i32)> {
    debug!("Running: docker {}", args.join(" "));

    let output = tokio::time::timeout(
        std::time::Duration::from_secs_f64(timeout_secs),
        Command::new("docker").args(args).output(),
    )
    .await
    .map_err(|_| DockerError::Timeout {
        seconds: timeout_secs,
        operation: format!("docker {}", args.join(" ")),
    })?
    .map_err(|e| DockerError::CommandFailed {
        message: format!("Failed to run docker command: {e}"),
        stderr: String::new(),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    if check && code != 0 {
        error!("Docker command failed: {stderr}");
        return Err(DockerError::CommandFailed {
            message: format!("docker {} exited with code {code}", args.join(" ")),
            stderr,
        });
    }

    Ok((stdout, stderr, code))
}

/// Manages Docker container lifecycle (create, start, stop, remove, inspect).
pub struct DockerDeployment {
    config: DockerConfig,
    container_name: String,
    container_id: Option<String>,
    host_port: u16,
    auth_token: String,
    started: bool,
    on_status: Box<dyn Fn(&str) + Send + Sync>,
}

impl DockerDeployment {
    /// Create a new deployment with the given config.
    pub fn new(config: DockerConfig) -> Result<Self> {
        let host_port = find_free_port()?;
        let auth_token = uuid::Uuid::new_v4().to_string();
        let container_name = format!("opendev-runtime-{}", &uuid::Uuid::new_v4().to_string()[..8]);

        Ok(Self {
            config,
            container_name,
            container_id: None,
            host_port,
            auth_token,
            started: false,
            on_status: Box::new(|_| {}),
        })
    }

    /// Set status callback.
    pub fn with_status_callback<F>(mut self, f: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_status = Box::new(f);
        self
    }

    /// Whether the deployment has started.
    pub fn is_started(&self) -> bool {
        self.started
    }

    /// The container ID (if started).
    pub fn container_id(&self) -> Option<&str> {
        self.container_id.as_deref()
    }

    /// The container name.
    pub fn container_name(&self) -> &str {
        &self.container_name
    }

    /// The host port mapped to the container server port.
    pub fn host_port(&self) -> u16 {
        self.host_port
    }

    /// The auth token for the in-container server.
    pub fn auth_token(&self) -> &str {
        &self.auth_token
    }

    /// Pull the Docker image according to the pull policy.
    pub async fn pull_image(&self) -> Result<()> {
        match self.config.pull_policy.as_str() {
            "never" => return Ok(()),
            "if-not-present" => {
                let (_, _, code) =
                    run_docker_command(&["image", "inspect", &self.config.image], 30.0, false)
                        .await?;
                if code == 0 {
                    info!("Image {} already exists locally", self.config.image);
                    return Ok(());
                }
            }
            _ => {} // "always" — fall through to pull
        }

        (self.on_status)(&format!("Pulling Docker image: {}", self.config.image));
        info!("Pulling Docker image: {}", self.config.image);

        run_docker_command(&["pull", &self.config.image], 600.0, true)
            .await
            .map_err(|e| DockerError::ImagePullFailed {
                image: self.config.image.clone(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Start the container in detached mode.
    pub async fn start_container(&mut self) -> Result<()> {
        (self.on_status)(&format!("Starting container: {}", self.container_name));

        let mut args: Vec<String> = vec![
            "run".into(),
            "--detach".into(),
            "--rm".into(),
            format!("--name={}", self.container_name),
            format!("--memory={}", self.config.memory),
            format!("--cpus={}", self.config.cpus),
            format!("--publish={}:{}", self.host_port, self.config.server_port),
        ];

        // Volume mounts
        for v in &self.config.volumes {
            let ro = if v.read_only { ":ro" } else { "" };
            args.push(format!(
                "--volume={}:{}{}",
                v.host_path, v.container_path, ro
            ));
        }

        // Environment variables
        let mut env: HashMap<String, String> = HashMap::new();
        env.insert("OPENDEV_AUTH_TOKEN".into(), self.auth_token.clone());
        env.insert("OPENDEV_PORT".into(), self.config.server_port.to_string());
        env.extend(self.config.environment.clone());
        for (k, v) in &env {
            args.push(format!("--env={k}={v}"));
        }

        // Image
        args.push(self.config.image.clone());

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let (stdout, _, _) = run_docker_command(&arg_refs, 300.0, true).await?;
        self.container_id = Some(stdout.trim().to_string());
        info!("Container started: {}", self.container_name);

        Ok(())
    }

    /// Full start flow: pull image + start container.
    pub async fn start(&mut self) -> Result<()> {
        if self.started {
            warn!("Deployment already started");
            return Ok(());
        }

        self.pull_image().await?;
        self.start_container().await?;
        self.started = true;

        (self.on_status)(&format!("Container ready on port {}", self.host_port));
        Ok(())
    }

    /// Stop and remove the container.
    pub async fn stop(&mut self) -> Result<()> {
        if !self.started && self.container_id.is_none() {
            return Ok(());
        }

        (self.on_status)("Stopping container...");
        info!("Stopping container: {}", self.container_name);

        if let Some(ref id) = self.container_id {
            // Graceful stop
            if let Err(e) = run_docker_command(&["stop", "-t", "5", id], 30.0, false).await {
                warn!("Error stopping container: {e}");
            }
            // Force remove
            let _ = run_docker_command(&["rm", "-f", id], 30.0, false).await;
            self.container_id = None;
        }

        self.started = false;
        info!("Container stopped");
        Ok(())
    }

    /// Inspect the container and return its status.
    pub async fn inspect(&self) -> Result<ContainerStatus> {
        let id = match &self.container_id {
            Some(id) => id.clone(),
            None => return Ok(ContainerStatus::Unknown),
        };

        let (stdout, _, code) = run_docker_command(
            &["inspect", "--format", "{{.State.Status}}", &id],
            10.0,
            false,
        )
        .await?;

        if code != 0 {
            return Ok(ContainerStatus::Unknown);
        }

        match stdout.trim() {
            "created" => Ok(ContainerStatus::Created),
            "running" => Ok(ContainerStatus::Running),
            "paused" => Ok(ContainerStatus::Paused),
            "exited" | "dead" => Ok(ContainerStatus::Stopped),
            "removing" => Ok(ContainerStatus::Removing),
            _ => Ok(ContainerStatus::Unknown),
        }
    }

    /// Remove the container forcefully.
    pub async fn remove(&mut self) -> Result<()> {
        if let Some(ref id) = self.container_id {
            run_docker_command(&["rm", "-f", id], 30.0, false).await?;
            self.container_id = None;
            self.started = false;
        }
        Ok(())
    }
}

impl Drop for DockerDeployment {
    fn drop(&mut self) {
        if let Some(ref id) = self.container_id {
            // Best-effort synchronous cleanup
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", id])
                .output();
        }
    }
}

#[cfg(test)]
#[path = "deployment_tests.rs"]
mod tests;
