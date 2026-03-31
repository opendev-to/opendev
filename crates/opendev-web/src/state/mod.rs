//! Shared application state.
//!
//! Thread-safe state shared between HTTP handlers and WebSocket connections.
//! Uses `tokio::sync::oneshot` channels for approval, ask-user, and plan-approval
//! notification so that waiting agent tasks are woken immediately on resolution
//! (no polling).

mod approvals;
mod bridge;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock, broadcast, mpsc, oneshot};

use opendev_config::ModelRegistry;
use opendev_history::SessionManager;
use opendev_http::UserStore;
use opendev_models::AppConfig;

/// WebSocket broadcast message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WsBroadcast {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub data: serde_json::Value,
    /// Monotonically increasing sequence number for ordering and gap detection.
    /// Defaults to 0 for backward compatibility with old frontends.
    #[serde(default)]
    pub seq: u64,
}

impl WsBroadcast {
    /// Create a new broadcast message. The `seq` field starts at 0 and is
    /// overwritten by [`AppState::broadcast`] before sending.
    pub fn new(msg_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            msg_type: msg_type.into(),
            data,
            seq: 0,
        }
    }
}

/// Shared application state wrapped in Arc for use with Axum.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

pub(super) struct AppStateInner {
    /// Session manager for persistence.
    pub(super) session_manager: RwLock<SessionManager>,
    /// Application configuration.
    pub(super) config: RwLock<AppConfig>,
    /// Working directory for the current project.
    pub(super) working_dir: String,
    /// Broadcast channel for WebSocket messages.
    pub(super) ws_tx: broadcast::Sender<WsBroadcast>,
    /// Pending approval requests: approval_id -> (metadata, oneshot sender).
    pub(super) pending_approvals: Mutex<HashMap<String, PendingApprovalSlot>>,
    /// Pending ask-user requests: request_id -> (metadata, oneshot sender).
    pub(super) pending_ask_users: Mutex<HashMap<String, PendingAskUserSlot>>,
    /// Pending plan approval requests: request_id -> (metadata, oneshot sender).
    pub(super) pending_plan_approvals: Mutex<HashMap<String, PendingPlanApprovalSlot>>,
    /// Current operation mode (normal/plan).
    pub(super) mode: RwLock<OperationMode>,
    /// Autonomy level.
    pub(super) autonomy_level: RwLock<String>,
    /// Interrupt flag.
    pub(super) interrupt_requested: Mutex<bool>,
    /// Running sessions: session_id -> status.
    pub(super) running_sessions: Mutex<HashMap<String, String>>,
    /// Live message injection queues: session_id -> bounded mpsc sender.
    pub(super) injection_queues: Mutex<HashMap<String, mpsc::Sender<String>>>,
    /// Agent executor (trait-object, set once on first query).
    pub(super) agent_executor: Mutex<Option<Arc<dyn AgentExecutor>>>,
    /// User store for authentication.
    pub(super) user_store: Arc<UserStore>,
    /// Model/provider registry from models.dev cache.
    pub(super) model_registry: RwLock<ModelRegistry>,
    /// Bridge mode state.
    pub(super) bridge: RwLock<BridgeState>,
    /// Monotonically increasing broadcast sequence counter.
    pub(super) broadcast_seq: AtomicU64,
    /// Ring buffer of recent broadcasts for client catch-up on reconnect.
    pub(super) recent_broadcasts: Mutex<VecDeque<WsBroadcast>>,
}

/// Bridge mode state: when the TUI owns agent execution and
/// the Web UI mirrors it.
#[derive(Debug, Default)]
pub(super) struct BridgeState {
    /// Session ID currently owned by the TUI bridge.
    pub(super) session_id: Option<String>,
    /// Whether bridge mode is active.
    pub(super) active: bool,
}

/// Operation mode for the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationMode {
    Normal,
    Plan,
}

impl std::fmt::Display for OperationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationMode::Normal => write!(f, "normal"),
            OperationMode::Plan => write!(f, "plan"),
        }
    }
}

/// Metadata for a pending approval request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingApproval {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub session_id: Option<String>,
}

/// Internal slot holding approval metadata and the oneshot sender.
pub(super) struct PendingApprovalSlot {
    pub meta: PendingApproval,
    pub tx: Option<oneshot::Sender<ApprovalResult>>,
}

/// Result sent through the oneshot channel when an approval is resolved.
#[derive(Debug, Clone)]
pub struct ApprovalResult {
    pub approved: bool,
    pub auto_approve: bool,
}

/// Metadata for a pending ask-user request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingAskUser {
    pub prompt: String,
    pub session_id: Option<String>,
}

/// Internal slot holding ask-user metadata and the oneshot sender.
pub(super) struct PendingAskUserSlot {
    pub meta: PendingAskUser,
    pub tx: Option<oneshot::Sender<AskUserResult>>,
}

/// Result sent through the oneshot channel when ask-user is resolved.
#[derive(Debug, Clone)]
pub struct AskUserResult {
    pub answers: Option<serde_json::Value>,
    pub cancelled: bool,
}

/// Metadata for a pending plan approval request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingPlanApproval {
    pub data: serde_json::Value,
    pub session_id: Option<String>,
}

/// Internal slot holding plan-approval metadata and the oneshot sender.
pub(super) struct PendingPlanApprovalSlot {
    pub meta: PendingPlanApproval,
    pub tx: Option<oneshot::Sender<PlanApprovalResult>>,
}

/// Result sent through the oneshot channel when a plan approval is resolved.
#[derive(Debug, Clone)]
pub struct PlanApprovalResult {
    pub action: String,
    pub feedback: String,
}

/// Trait for agent execution -- injected into AppState for testability.
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync + 'static {
    /// Execute a query for a given session. Called as a background task.
    async fn execute_query(
        &self,
        message: String,
        session_id: String,
        state: AppState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Injection queue capacity per session.
const INJECTION_QUEUE_CAPACITY: usize = 10;

/// Maximum number of recent broadcasts kept for client catch-up on reconnect.
const RING_BUFFER_CAPACITY: usize = 1000;

impl AppState {
    /// Create a new AppState without per-directory isolation.
    pub fn new(
        session_manager: SessionManager,
        config: AppConfig,
        working_dir: String,
        user_store: UserStore,
        model_registry: ModelRegistry,
    ) -> Self {
        let (ws_tx, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(AppStateInner {
                session_manager: RwLock::new(session_manager),
                config: RwLock::new(config),
                working_dir,
                ws_tx,
                pending_approvals: Mutex::new(HashMap::new()),
                pending_ask_users: Mutex::new(HashMap::new()),
                pending_plan_approvals: Mutex::new(HashMap::new()),
                mode: RwLock::new(OperationMode::Normal),
                autonomy_level: RwLock::new("Manual".to_string()),
                interrupt_requested: Mutex::new(false),
                running_sessions: Mutex::new(HashMap::new()),
                injection_queues: Mutex::new(HashMap::new()),
                agent_executor: Mutex::new(None),
                user_store: Arc::new(user_store),
                model_registry: RwLock::new(model_registry),
                bridge: RwLock::new(BridgeState::default()),
                broadcast_seq: AtomicU64::new(1),
                recent_broadcasts: Mutex::new(VecDeque::with_capacity(RING_BUFFER_CAPACITY)),
            }),
        }
    }

    // --- Accessors ---

    /// Get a read guard for the session manager.
    pub async fn session_manager(&self) -> tokio::sync::RwLockReadGuard<'_, SessionManager> {
        self.inner.session_manager.read().await
    }

    /// Get a write guard for the session manager.
    pub async fn session_manager_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, SessionManager> {
        self.inner.session_manager.write().await
    }

    /// Get the current session ID (if a session is loaded).
    pub async fn current_session_id(&self) -> Option<String> {
        self.inner
            .session_manager
            .read()
            .await
            .current_session()
            .map(|s| s.id.clone())
    }

    /// Get a read guard for the app config.
    pub async fn config(&self) -> tokio::sync::RwLockReadGuard<'_, AppConfig> {
        self.inner.config.read().await
    }

    /// Get a write guard for the app config.
    pub async fn config_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, AppConfig> {
        self.inner.config.write().await
    }

    /// Get the working directory.
    pub fn working_dir(&self) -> &str {
        &self.inner.working_dir
    }

    // --- User store ---

    /// Get a reference to the user store.
    pub fn user_store(&self) -> &UserStore {
        &self.inner.user_store
    }

    // --- Model registry ---

    /// Get a read guard for the model registry.
    pub async fn model_registry(&self) -> tokio::sync::RwLockReadGuard<'_, ModelRegistry> {
        self.inner.model_registry.read().await
    }

    /// Get a write guard for the model registry.
    pub async fn model_registry_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ModelRegistry> {
        self.inner.model_registry.write().await
    }

    // --- WebSocket ---

    /// Get a clone of the broadcast sender.
    pub fn ws_sender(&self) -> broadcast::Sender<WsBroadcast> {
        self.inner.ws_tx.clone()
    }

    /// Subscribe to WebSocket broadcasts.
    pub fn ws_subscribe(&self) -> broadcast::Receiver<WsBroadcast> {
        self.inner.ws_tx.subscribe()
    }

    /// Broadcast a message to all WebSocket subscribers.
    ///
    /// Assigns a monotonically increasing sequence number before sending.
    pub fn broadcast(&self, mut msg: WsBroadcast) {
        msg.seq = self.inner.broadcast_seq.fetch_add(1, Ordering::Relaxed);
        // Store in ring buffer for catch-up on reconnect.
        if let Ok(mut buf) = self.inner.recent_broadcasts.try_lock() {
            if buf.len() >= RING_BUFFER_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(msg.clone());
        }
        // Ignore send errors (no subscribers is fine).
        let _ = self.inner.ws_tx.send(msg);
    }

    /// Get the current broadcast sequence number (next to be assigned).
    pub fn current_broadcast_seq(&self) -> u64 {
        self.inner.broadcast_seq.load(Ordering::Relaxed)
    }

    /// Get all broadcasts with seq > `last_seq` from the ring buffer.
    /// Returns `None` if the requested seq is too old (no longer in buffer).
    pub async fn catch_up_since(&self, last_seq: u64) -> Option<Vec<WsBroadcast>> {
        let buf = self.inner.recent_broadcasts.lock().await;
        // If buffer is empty, there is nothing to catch up on.
        if buf.is_empty() {
            return Some(vec![]);
        }
        // Check if the requested seq is still in the buffer.
        if let Some(oldest) = buf.front()
            && last_seq < oldest.seq.saturating_sub(1)
        {
            return None; // Gap too large, client needs full sync
        }
        Some(buf.iter().filter(|m| m.seq > last_seq).cloned().collect())
    }

    // --- Mode / settings ---

    /// Get the current operation mode.
    pub async fn mode(&self) -> OperationMode {
        *self.inner.mode.read().await
    }

    /// Set the operation mode.
    pub async fn set_mode(&self, mode: OperationMode) {
        *self.inner.mode.write().await = mode;
    }

    // --- Autonomy level ---

    /// Get the current autonomy level.
    pub async fn autonomy_level(&self) -> String {
        self.inner.autonomy_level.read().await.clone()
    }

    /// Set the autonomy level.
    pub async fn set_autonomy_level(&self, level: String) {
        *self.inner.autonomy_level.write().await = level;
    }

    // --- Interrupt ---

    /// Request an interrupt.
    ///
    /// Also denies all pending approvals, ask-user, and plan-approval requests
    /// by sending rejection through their oneshot channels so blocked tasks wake up.
    pub async fn request_interrupt(&self) {
        *self.inner.interrupt_requested.lock().await = true;

        // Deny all pending approvals.
        {
            let mut approvals = self.inner.pending_approvals.lock().await;
            for (_id, slot) in approvals.iter_mut() {
                if let Some(tx) = slot.tx.take() {
                    let _ = tx.send(ApprovalResult {
                        approved: false,
                        auto_approve: false,
                    });
                }
            }
            approvals.clear();
        }

        // Cancel all pending ask-user requests.
        {
            let mut ask_users = self.inner.pending_ask_users.lock().await;
            for (_id, slot) in ask_users.iter_mut() {
                if let Some(tx) = slot.tx.take() {
                    let _ = tx.send(AskUserResult {
                        answers: None,
                        cancelled: true,
                    });
                }
            }
            ask_users.clear();
        }

        // Reject all pending plan approvals.
        {
            let mut plan_approvals = self.inner.pending_plan_approvals.lock().await;
            for (_id, slot) in plan_approvals.iter_mut() {
                if let Some(tx) = slot.tx.take() {
                    let _ = tx.send(PlanApprovalResult {
                        action: "reject".to_string(),
                        feedback: "Interrupted".to_string(),
                    });
                }
            }
            plan_approvals.clear();
        }
    }

    /// Clear the interrupt flag.
    pub async fn clear_interrupt(&self) {
        *self.inner.interrupt_requested.lock().await = false;
    }

    /// Check if interrupt has been requested.
    pub async fn is_interrupt_requested(&self) -> bool {
        *self.inner.interrupt_requested.lock().await
    }

    // --- Running sessions ---

    /// Mark a session as running.
    pub async fn set_session_running(&self, session_id: String) {
        self.inner
            .running_sessions
            .lock()
            .await
            .insert(session_id, "running".to_string());
    }

    /// Mark a session as idle.
    pub async fn set_session_idle(&self, session_id: &str) {
        self.inner.running_sessions.lock().await.remove(session_id);
    }

    /// Check if a session is running.
    pub async fn is_session_running(&self, session_id: &str) -> bool {
        self.inner
            .running_sessions
            .lock()
            .await
            .contains_key(session_id)
    }

    // --- Git branch ---

    /// Get the git branch for the working directory.
    pub fn git_branch(&self) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.inner.working_dir)
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
