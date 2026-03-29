//! Data models for Docker runtime configuration and communication.
//!
//! Ports the Python `models.py` plus configuration types from `deployment.py`.

use serde::{Deserialize, Serialize};

// =============================================================================
// Configuration
// =============================================================================

/// Whether to interact with a local or remote Docker daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum RuntimeType {
    #[default]
    Local,
    Remote,
}

/// A single volume mount specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Host path.
    pub host_path: String,
    /// Container path.
    pub container_path: String,
    /// Read-only flag.
    #[serde(default)]
    pub read_only: bool,
}

/// A single port mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortMapping {
    /// Port on the host.
    pub host_port: u16,
    /// Port inside the container.
    pub container_port: u16,
    /// Protocol (tcp/udp).
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "tcp".into()
}

/// Status of a Docker container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ContainerStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Removing,
    #[default]
    Unknown,
}

/// Specification for creating a container (image, resources, mounts, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSpec {
    /// Docker image name (e.g. `python:3.11`).
    pub image: String,
    /// Memory limit (e.g. `4g`).
    #[serde(default = "default_memory")]
    pub memory: String,
    /// CPU limit (e.g. `4`).
    #[serde(default = "default_cpus")]
    pub cpus: String,
    /// Docker network mode.
    #[serde(default = "default_network")]
    pub network_mode: String,
    /// Volume mounts.
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    /// Port mappings.
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    /// Extra environment variables.
    #[serde(default)]
    pub environment: std::collections::HashMap<String, String>,
    /// Entrypoint command override.
    pub entrypoint: Option<String>,
    /// Command to run.
    pub command: Option<Vec<String>>,
}

fn default_memory() -> String {
    "4g".into()
}
fn default_cpus() -> String {
    "4".into()
}
fn default_network() -> String {
    "bridge".into()
}

/// Top-level Docker configuration (mirrors Python `DockerConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerConfig {
    /// Container image.
    #[serde(default = "default_image")]
    pub image: String,
    /// Memory limit.
    #[serde(default = "default_memory")]
    pub memory: String,
    /// CPU limit.
    #[serde(default = "default_cpus")]
    pub cpus: String,
    /// Docker network mode.
    #[serde(default = "default_network")]
    pub network_mode: String,
    /// Maximum seconds to wait for the container to become ready.
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout: f64,
    /// Image pull policy: `always`, `never`, or `if-not-present`.
    #[serde(default = "default_pull_policy")]
    pub pull_policy: String,
    /// Port the server listens on inside the container.
    #[serde(default = "default_server_port")]
    pub server_port: u16,
    /// Extra environment variables.
    #[serde(default)]
    pub environment: std::collections::HashMap<String, String>,
    /// Shell init command prepended to every command.
    #[serde(default)]
    pub shell_init: String,
    /// Volume mounts.
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    /// Runtime type (local or remote Docker daemon).
    #[serde(default)]
    pub runtime_type: RuntimeType,
    /// Remote host for SSH-based Docker access.
    pub remote_host: Option<String>,
    /// SSH user for remote Docker.
    pub remote_user: Option<String>,
    /// SSH key path.
    pub ssh_key_path: Option<String>,
}

fn default_image() -> String {
    "python:3.11".into()
}
fn default_startup_timeout() -> f64 {
    120.0
}
fn default_pull_policy() -> String {
    "if-not-present".into()
}
fn default_server_port() -> u16 {
    8000
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            image: default_image(),
            memory: default_memory(),
            cpus: default_cpus(),
            network_mode: default_network(),
            startup_timeout: default_startup_timeout(),
            pull_policy: default_pull_policy(),
            server_port: default_server_port(),
            environment: Default::default(),
            shell_init: String::new(),
            volumes: Vec::new(),
            runtime_type: RuntimeType::default(),
            remote_host: None,
            remote_user: None,
            ssh_key_path: None,
        }
    }
}

// =============================================================================
// Session management request/response
// =============================================================================

/// Request to create a new bash session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default = "default_session_name")]
    pub session: String,
    #[serde(default = "default_startup_timeout_session")]
    pub startup_timeout: f64,
}

fn default_session_name() -> String {
    "default".into()
}
fn default_startup_timeout_session() -> f64 {
    10.0
}

impl Default for CreateSessionRequest {
    fn default() -> Self {
        Self {
            session: default_session_name(),
            startup_timeout: default_startup_timeout_session(),
        }
    }
}

/// Response after creating a bash session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResponse {
    pub success: bool,
    pub session: String,
    #[serde(default)]
    pub message: String,
}

/// Request to close a bash session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseSessionRequest {
    #[serde(default = "default_session_name")]
    pub session: String,
}

impl Default for CloseSessionRequest {
    fn default() -> Self {
        Self {
            session: default_session_name(),
        }
    }
}

/// Response after closing a bash session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseSessionResponse {
    pub success: bool,
    #[serde(default)]
    pub message: String,
}

// =============================================================================
// Command execution
// =============================================================================

/// How to handle non-zero exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum CheckMode {
    /// Raise an error on non-zero exit.
    Raise,
    /// Return the result silently.
    #[default]
    Silent,
    /// Skip exit-code checking entirely.
    Ignore,
}

/// Action to execute in a bash session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashAction {
    pub command: String,
    #[serde(default = "default_session_name")]
    pub session: String,
    #[serde(default = "default_timeout")]
    pub timeout: f64,
    #[serde(default)]
    pub check: CheckMode,
}

fn default_timeout() -> f64 {
    120.0
}

/// Observation/result from executing a bash action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashObservation {
    #[serde(default)]
    pub output: String,
    pub exit_code: Option<i32>,
    pub failure_reason: Option<String>,
}

// =============================================================================
// File operations
// =============================================================================

/// Request to read a file from the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileRequest {
    pub path: String,
}

/// Response with file contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileResponse {
    pub success: bool,
    #[serde(default)]
    pub content: String,
    pub error: Option<String>,
}

/// Request to write a file in the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
}

/// Response after writing a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileResponse {
    pub success: bool,
    pub error: Option<String>,
}

// =============================================================================
// Health check
// =============================================================================

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsAliveResponse {
    #[serde(default = "default_status_ok")]
    pub status: String,
    #[serde(default)]
    pub message: String,
}

fn default_status_ok() -> String {
    "ok".into()
}

impl Default for IsAliveResponse {
    fn default() -> Self {
        Self {
            status: default_status_ok(),
            message: String::new(),
        }
    }
}

/// Serialized exception for transfer over HTTP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionTransfer {
    pub message: String,
    pub class_name: String,
    pub module: String,
    #[serde(default)]
    pub traceback: String,
    #[serde(default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// =============================================================================
// Tool handler result
// =============================================================================

/// Generic result from a Docker tool handler operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
