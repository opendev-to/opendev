//! Sandbox lifecycle management using microsandbox.

use std::collections::HashMap;

use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::errors::{Result, SandboxError};
use opendev_models::config::SandboxConfig;

/// Wrapper around a microsandbox Python sandbox with lifecycle management.
pub struct MicroSandbox {
    session_id: String,
    started: bool,
    // Will hold: microsandbox::PythonSandbox once we wire the dependency
}

impl MicroSandbox {
    /// Create a new sandbox instance.
    ///
    /// Automatically starts the microsandbox server if not already running.
    /// Returns `ServerUnavailable` if the runtime is not installed or fails to start.
    pub async fn create(session_id: &str, _config: &SandboxConfig) -> Result<Self> {
        // Check platform support first.
        if let Some(reason) = crate::runtime::platform_availability() {
            return Err(SandboxError::ServerUnavailable(reason));
        }

        // Ensure server is running (auto-starts if needed).
        crate::runtime::ensure_server_running().await?;

        info!(session_id, "Creating sandbox");

        // TODO: PythonSandbox::create() call
        // For now, return a placeholder that will be wired when we
        // integrate the microsandbox SDK's PythonSandbox type.

        Ok(Self {
            session_id: session_id.to_string(),
            started: false,
        })
    }

    /// Start the sandbox with resource limits from config.
    pub async fn start(&mut self, _config: &SandboxConfig) -> Result<()> {
        info!(session_id = %self.session_id, "Starting sandbox");

        // TODO: sandbox.start(StartOptions { image, memory, cpus, timeout })

        self.started = true;
        Ok(())
    }

    /// Execute Python code in the sandbox. Returns stdout or error string.
    pub async fn run_code(&self, code: &str) -> Result<String> {
        if !self.started {
            return Err(SandboxError::Execution("Sandbox not started".to_string()));
        }

        // TODO: sandbox.run(code).await, capture output/error

        let _ = code;
        Ok(String::new())
    }

    /// Inject a string variable into the sandbox environment.
    pub async fn inject_variable(&self, name: &str, content: &str) -> Result<()> {
        // Escape triple quotes in content to avoid Python syntax errors.
        let escaped = content.replace("\\", "\\\\").replace("'''", "\\'\\'\\'");
        let code = format!("{name} = '''{escaped}'''");
        self.run_code(&code).await?;
        Ok(())
    }

    /// Stop the sandbox and release resources.
    pub async fn stop(&mut self) -> Result<()> {
        if self.started {
            info!(session_id = %self.session_id, "Stopping sandbox");
            // TODO: sandbox.stop()
            self.started = false;
        }
        Ok(())
    }

    /// Whether the sandbox is currently running.
    pub fn is_started(&self) -> bool {
        self.started
    }

    /// The session identifier for this sandbox.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

impl std::fmt::Debug for MicroSandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicroSandbox")
            .field("session_id", &self.session_id)
            .field("started", &self.started)
            .finish()
    }
}

/// Pool of reusable sandboxes keyed by session ID.
pub struct SandboxPool {
    sandboxes: Mutex<HashMap<String, MicroSandbox>>,
}

impl SandboxPool {
    pub fn new() -> Self {
        Self {
            sandboxes: Mutex::new(HashMap::new()),
        }
    }

    /// Get an existing sandbox or create a new one for the given session.
    pub async fn get_or_create(&self, session_id: &str, config: &SandboxConfig) -> Result<()> {
        let mut sandboxes = self.sandboxes.lock().await;
        if sandboxes.contains_key(session_id) {
            return Ok(());
        }

        let mut sandbox = MicroSandbox::create(session_id, config).await?;
        sandbox.start(config).await?;
        sandboxes.insert(session_id.to_string(), sandbox);
        Ok(())
    }

    /// Stop and remove all sandboxes. Called on agent shutdown.
    pub async fn stop_all(&self) {
        let mut sandboxes = self.sandboxes.lock().await;
        for (id, sandbox) in sandboxes.iter_mut() {
            if let Err(e) = sandbox.stop().await {
                warn!(session_id = %id, error = %e, "Failed to stop sandbox");
            }
        }
        sandboxes.clear();
    }
}

impl Default for SandboxPool {
    fn default() -> Self {
        Self::new()
    }
}
