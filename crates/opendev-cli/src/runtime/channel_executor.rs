//! AgentExecutor implementation for channel adapters (Telegram, etc.).
//!
//! Creates and manages per-session `AgentRuntime` instances so that each
//! channel conversation gets its own isolated agent session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{error, info};

use opendev_channels::error::{ChannelError, ChannelResult};
use opendev_channels::router::AgentExecutor;
use opendev_history::SessionManager;
use opendev_models::AppConfig;

use super::AgentRuntime;

/// An `AgentExecutor` that creates per-session `AgentRuntime` instances.
pub struct ChannelAgentExecutor {
    config: AppConfig,
    working_dir: PathBuf,
    system_prompt: String,
    sessions: Mutex<HashMap<String, Arc<Mutex<AgentRuntime>>>>,
}

impl ChannelAgentExecutor {
    pub fn new(config: AppConfig, working_dir: &Path, system_prompt: String) -> Self {
        Self {
            config,
            working_dir: working_dir.to_path_buf(),
            system_prompt,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    async fn get_or_create_runtime(
        &self,
        session_id: &str,
    ) -> ChannelResult<Arc<Mutex<AgentRuntime>>> {
        let mut sessions = self.sessions.lock().await;

        if let Some(runtime) = sessions.get(session_id) {
            return Ok(Arc::clone(runtime));
        }

        let paths = opendev_config::Paths::new(Some(self.working_dir.clone()));
        let session_dir = paths.project_sessions_dir(&self.working_dir);

        let mut session_manager = SessionManager::new(session_dir)
            .map_err(|e| ChannelError::Session(format!("failed to create session manager: {e}")))?;
        session_manager.create_session();

        let runtime = AgentRuntime::new(self.config.clone(), &self.working_dir, session_manager)
            .map_err(|e| {
                ChannelError::AgentError(format!("failed to create agent runtime: {e}"))
            })?;

        let runtime = Arc::new(Mutex::new(runtime));
        sessions.insert(session_id.to_string(), Arc::clone(&runtime));

        info!(session_id, "Created new agent runtime for channel session");
        Ok(runtime)
    }
}

#[async_trait]
impl AgentExecutor for ChannelAgentExecutor {
    async fn execute(&self, session_id: &str, message_text: &str) -> ChannelResult<String> {
        let runtime = self.get_or_create_runtime(session_id).await?;
        let mut runtime = runtime.lock().await;

        match runtime
            .run_query(message_text, &self.system_prompt, None, None, false)
            .await
        {
            Ok(result) => Ok(result.content),
            Err(e) => {
                error!(session_id, error = %e, "Agent execution failed");
                Err(ChannelError::AgentError(e.to_string()))
            }
        }
    }
}
