//! Background polling loop for Telegram getUpdates.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::router::{InboundMessage, MessageRouter};
use tokio::sync::{RwLock, watch};
use tracing::{debug, error, info, warn};

use super::DmPolicy;
use super::api::TelegramApi;
use super::remote::{ApprovalResponse, RemoteCommand, RemoteEvent, RemoteSessionBridge};
use super::remote_claim::RemoteSessionClaim;
use super::types::{
    EditMessageTextRequest, InlineKeyboardButton, InlineKeyboardMarkup, Message,
    SendMessageRequest, SendMessageWithMarkupRequest,
};

/// Minimum characters before sending the first draft message (better push notification UX).
const MIN_INITIAL_CHARS: usize = 30;
/// Throttle interval between message edits in milliseconds.
const THROTTLE_MS: u64 = 1000;

/// Background poller that fetches Telegram updates and routes them to the agent.
pub struct TelegramPoller {
    pub(super) api: Arc<TelegramApi>,
    pub(super) router: Arc<MessageRouter>,
    pub(super) bot_username: String,
    pub(super) bot_id: i64,
    pub(super) group_mention_only: bool,
    pub(super) dm_policy: DmPolicy,
    pub(super) allowed_users: Arc<RwLock<HashSet<String>>>,
    pub(super) remote_claim: Option<Arc<RemoteSessionClaim>>,
}

impl TelegramPoller {
    /// Spawn the polling loop as a background tokio task.
    /// Returns a shutdown sender — drop or send `true` to stop polling.
    pub fn spawn(self: Arc<Self>) -> watch::Sender<bool> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        tokio::spawn(async move {
            self.run(shutdown_rx).await;
        });
        shutdown_tx
    }

    /// Spawn the polling loop in remote-control mode.
    ///
    /// Instead of routing messages to isolated agent sessions, this mode
    /// forwards messages and callback queries through the `RemoteSessionBridge`.
    pub fn spawn_remote(self: Arc<Self>, bridge: Arc<RemoteSessionBridge>) -> watch::Sender<bool> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        tokio::spawn(async move {
            self.run_remote(shutdown_rx, bridge).await;
        });
        shutdown_tx
    }

    /// Add a user to the runtime allowlist (called when pairing is approved).
    pub async fn approve_user(&self, user_id: &str) {
        let mut allowed = self.allowed_users.write().await;
        allowed.insert(user_id.to_string());
        info!("Telegram: approved user {}", user_id);
    }

    /// Remove a user from the runtime allowlist.
    pub async fn remove_user(&self, user_id: &str) {
        let mut allowed = self.allowed_users.write().await;
        allowed.remove(user_id);
        info!("Telegram: removed user {}", user_id);
    }

    /// Check if a user is in the allowlist.
    async fn is_user_allowed(&self, user_id: &str) -> bool {
        let allowed = self.allowed_users.read().await;
        allowed.contains(user_id)
    }

    async fn run(self: Arc<Self>, mut shutdown_rx: watch::Receiver<bool>) {
        let mut offset: i64 = 0;

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    debug!("Telegram poller received shutdown signal");
                    break;
                }
                result = self.api.get_updates(offset) => {
                    match result {
                        Ok(updates) => {
                            if !updates.is_empty() {
                                info!("Telegram: received {} update(s)", updates.len());
                            }
                            for update in updates {
                                offset = update.update_id + 1;
                                if let Some(msg) = update.message {
                                    debug!(
                                        "Telegram: message from user={} chat={} text={:?}",
                                        msg.from.as_ref().map(|u| u.id).unwrap_or(0),
                                        msg.chat.id,
                                        msg.text.as_deref().unwrap_or("(no text)")
                                    );
                                    let poller = Arc::clone(&self);
                                    tokio::spawn(async move {
                                        poller.handle_message(&msg).await;
                                    });
                                } else {
                                    debug!("Telegram: update {} has no message field", update.update_id);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Telegram getUpdates error: {}", e);
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        }
    }

    async fn handle_message(&self, msg: &Message) {
        let text = match &msg.text {
            Some(t) => t.clone(),
            None => return, // ignore non-text messages
        };

        let from = match &msg.from {
            Some(u) => u,
            None => return,
        };

        let is_private = msg.chat.chat_type == "private";
        let sender_id = from.id.to_string();

        // ── DM access control (pairing) ──
        if is_private && self.dm_policy != DmPolicy::Open && !self.is_user_allowed(&sender_id).await
        {
            match self.dm_policy {
                DmPolicy::Pairing => {
                    self.send_pairing_challenge(msg, &sender_id).await;
                    return;
                }
                DmPolicy::Allowlist => {
                    debug!(
                        "Telegram: ignoring message from non-allowed user {}",
                        sender_id
                    );
                    return;
                }
                DmPolicy::Open => unreachable!(),
            }
        }

        // ── Group chat filtering ──
        if !is_private && self.group_mention_only {
            let mention = format!("@{}", self.bot_username);
            let is_mention = text.contains(&mention);
            let is_reply_to_bot = msg
                .reply_to_message
                .as_ref()
                .and_then(|r| r.from.as_ref())
                .is_some_and(|u| u.id == self.bot_id);

            if !is_mention && !is_reply_to_bot {
                debug!("Telegram: skipping group message (no mention/reply)");
                return;
            }
        }

        // Strip @mention from text
        let mention = format!("@{}", self.bot_username);
        let clean_text = text.replace(&mention, "").trim().to_string();

        // Handle built-in commands locally
        if clean_text == "/start" || clean_text == "/help" {
            let help_text = "I'm an OpenDev AI assistant. Send me a message to get started.";
            let _ = self
                .api
                .send_message(SendMessageRequest {
                    chat_id: msg.chat.id,
                    text: help_text.to_string(),
                    parse_mode: None,
                    reply_to_message_id: None,
                })
                .await;
            return;
        }

        // Skip empty messages after stripping
        if clean_text.is_empty() {
            return;
        }

        let chat_id = msg.chat.id;
        // Send typing indicator and keep it alive while processing
        let api_for_typing = Arc::clone(&self.api);
        let typing_cancel = tokio_util::sync::CancellationToken::new();
        let typing_token = typing_cancel.clone();
        tokio::spawn(async move {
            loop {
                let _ = api_for_typing.send_chat_action(chat_id, "typing").await;
                tokio::select! {
                    _ = typing_token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(4)) => {}
                }
            }
        });

        // Create streaming channel
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Spawn the draft stream editor task (OpenClaw-style progressive editing)
        let api_for_draft = Arc::clone(&self.api);
        let draft_handle = tokio::spawn(Self::run_draft_stream(api_for_draft, chat_id, chunk_rx));

        // Build metadata with chat_id for delivery context passthrough
        let mut metadata = HashMap::new();
        metadata.insert("chat_id".to_string(), serde_json::json!(chat_id));
        metadata.insert("message_id".to_string(), serde_json::json!(msg.message_id));
        if let Some(ref username) = from.username {
            metadata.insert("username".to_string(), serde_json::json!(username));
        }

        let chat_type = if is_private { "direct" } else { "group" };

        let inbound = InboundMessage {
            channel: "telegram".to_string(),
            user_id: sender_id,
            thread_id: Some(msg.chat.id.to_string()),
            text: clean_text,
            timestamp: chrono::DateTime::from_timestamp(msg.date, 0)
                .unwrap_or_else(chrono::Utc::now),
            chat_type: chat_type.to_string(),
            reply_to_message_id: None,
            metadata,
        };

        debug!(
            "Telegram: routing message to agent (text={:?})",
            inbound.text
        );

        // Execute with streaming — chunks flow to the draft stream task
        let result = self
            .router
            .handle_inbound_streaming(inbound, chunk_tx)
            .await;

        // Wait for draft stream to finish flushing
        let draft_message_id = draft_handle.await.unwrap_or(None);

        // Stop typing indicator
        typing_cancel.cancel();

        // ── Final delivery ──
        // Convert the final response to Telegram HTML and send/edit
        match result {
            Ok(final_text) => {
                let trimmed = final_text.trim();
                if trimmed.is_empty() {
                    if let Some(mid) = draft_message_id {
                        let _ = self
                            .api
                            .edit_message_text(EditMessageTextRequest {
                                chat_id,
                                message_id: mid,
                                text: "Done.".to_string(),
                                parse_mode: None,
                                reply_markup: None,
                            })
                            .await;
                    }
                    return;
                }

                // Convert markdown to Telegram HTML
                let html = super::format::markdown_to_telegram_html(trimmed);

                // Split into chunks respecting HTML tag boundaries
                let chunks = super::format::split_telegram_html(&html, 4000);

                for (i, chunk) in chunks.iter().enumerate() {
                    if i == 0 {
                        if let Some(mid) = draft_message_id {
                            // Edit the draft message with final HTML content
                            let edit_result = self
                                .api
                                .edit_message_text(EditMessageTextRequest {
                                    chat_id,
                                    message_id: mid,
                                    text: chunk.clone(),
                                    parse_mode: Some("HTML".to_string()),
                                    reply_markup: None,
                                })
                                .await;

                            // Fall back to plain text if HTML parsing fails
                            if edit_result.is_err() {
                                debug!("Telegram: HTML edit failed, falling back to plain text");
                                let _ = self
                                    .api
                                    .edit_message_text(EditMessageTextRequest {
                                        chat_id,
                                        message_id: mid,
                                        text: trimmed.to_string(),
                                        parse_mode: None,
                                        reply_markup: None,
                                    })
                                    .await;
                                return;
                            }
                        } else {
                            // No draft was sent (very short response) — send fresh
                            let send_result = self
                                .api
                                .send_message(SendMessageRequest {
                                    chat_id,
                                    text: chunk.clone(),
                                    parse_mode: Some("HTML".to_string()),
                                    reply_to_message_id: None,
                                })
                                .await;

                            if send_result.is_err() {
                                let _ = self
                                    .api
                                    .send_message(SendMessageRequest {
                                        chat_id,
                                        text: trimmed.to_string(),
                                        parse_mode: None,
                                        reply_to_message_id: None,
                                    })
                                    .await;
                                return;
                            }
                        }
                    } else {
                        // Additional chunks — send as new messages
                        let send_result = self
                            .api
                            .send_message(SendMessageRequest {
                                chat_id,
                                text: chunk.clone(),
                                parse_mode: Some("HTML".to_string()),
                                reply_to_message_id: None,
                            })
                            .await;

                        if send_result.is_err() {
                            // Fall back to plain text for this chunk
                            let plain_chunk = strip_html_tags(chunk);
                            let _ = self
                                .api
                                .send_message(SendMessageRequest {
                                    chat_id,
                                    text: plain_chunk,
                                    parse_mode: None,
                                    reply_to_message_id: None,
                                })
                                .await;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Telegram: agent execution failed: {}", e);
                let error_text = format!("Error: {}", e);
                if let Some(mid) = draft_message_id {
                    let _ = self
                        .api
                        .edit_message_text(EditMessageTextRequest {
                            chat_id,
                            message_id: mid,
                            text: error_text,
                            parse_mode: None,
                            reply_markup: None,
                        })
                        .await;
                } else {
                    let _ = self
                        .api
                        .send_message(SendMessageRequest {
                            chat_id,
                            text: error_text,
                            parse_mode: None,
                            reply_to_message_id: None,
                        })
                        .await;
                }
            }
        }
    }

    /// OpenClaw-style draft stream: progressively edits a Telegram message
    /// as LLM tokens arrive. Waits for `MIN_INITIAL_CHARS` before sending the
    /// first message, then throttles edits at `THROTTLE_MS` intervals.
    ///
    /// Returns the message_id of the draft (or None if nothing was sent).
    async fn run_draft_stream(
        api: Arc<TelegramApi>,
        chat_id: i64,
        mut chunk_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    ) -> Option<i64> {
        let mut accumulated = String::new();
        let mut draft_message_id: Option<i64> = None;
        let mut last_edit = tokio::time::Instant::now();
        let mut pending = false;

        loop {
            let chunk =
                tokio::time::timeout(Duration::from_millis(THROTTLE_MS), chunk_rx.recv()).await;

            match chunk {
                Ok(Some(text)) => {
                    accumulated.push_str(&text);
                    pending = true;
                }
                Ok(None) => break, // channel closed — done
                Err(_) => {}       // timeout — flush below
            }

            if !pending || accumulated.trim().is_empty() {
                continue;
            }

            let should_edit = last_edit.elapsed() >= Duration::from_millis(THROTTLE_MS);

            if let Some(mid) = draft_message_id {
                if should_edit {
                    // Subsequent edits with cursor
                    let display = format!("{}▍", accumulated.trim());
                    let _ = api
                        .edit_message_text(EditMessageTextRequest {
                            chat_id,
                            message_id: mid,
                            text: display,
                            parse_mode: None,
                            reply_markup: None,
                        })
                        .await;
                    last_edit = tokio::time::Instant::now();
                    pending = false;
                }
            } else if accumulated.trim().len() >= MIN_INITIAL_CHARS {
                // First send: wait until we have enough content
                let display = format!("{}▍", accumulated.trim());
                match api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: display,
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await
                {
                    Ok(m) => {
                        draft_message_id = Some(m.message_id);
                        last_edit = tokio::time::Instant::now();
                        pending = false;
                    }
                    Err(e) => {
                        error!("Telegram: failed to send draft: {}", e);
                    }
                }
            }
        }

        // Final flush — remove cursor, show accumulated text as-is
        if let Some(mid) = draft_message_id
            && pending
            && !accumulated.trim().is_empty()
        {
            let _ = api
                .edit_message_text(EditMessageTextRequest {
                    chat_id,
                    message_id: mid,
                    text: accumulated.trim().to_string(),
                    parse_mode: None,
                    reply_markup: None,
                })
                .await;
        }

        draft_message_id
    }

    /// Send a pairing challenge to an unknown user.
    async fn send_pairing_challenge(&self, msg: &Message, sender_id: &str) {
        let sender_name = msg
            .from
            .as_ref()
            .map(|u| {
                u.username
                    .as_ref()
                    .map(|n| format!("@{n}"))
                    .unwrap_or_else(|| u.first_name.clone())
            })
            .unwrap_or_else(|| "Unknown".to_string());

        warn!(
            "Telegram: pairing request from {} (ID: {})",
            sender_name, sender_id
        );

        let challenge_text = format!(
            "🔒 Access required.\n\n\
             Your Telegram ID: `{}`\n\n\
             Ask the bot owner to approve you:\n\
             ```\nopendev channel pair {}\n```",
            sender_id, sender_id,
        );

        let _ = self
            .api
            .send_message(SendMessageRequest {
                chat_id: msg.chat.id,
                text: challenge_text,
                parse_mode: Some("Markdown".to_string()),
                reply_to_message_id: None,
            })
            .await;
    }
}

/// State tracked per remote-control chat.
struct RemoteChatState {
    chat_id: i64,
    /// Message ID of the current status message (edited in place).
    status_message_id: Option<i64>,
    /// Whether the agent is currently processing.
    agent_active: bool,
    /// Accumulated text for streaming display.
    accumulated_text: String,
    /// Last time the status message was edited (throttle).
    last_edit: tokio::time::Instant,
    /// Current context usage percentage.
    context_usage: f64,
}

impl RemoteChatState {
    fn new(chat_id: i64) -> Self {
        Self {
            chat_id,
            status_message_id: None,
            agent_active: false,
            accumulated_text: String::new(),
            last_edit: tokio::time::Instant::now(),
            context_usage: 0.0,
        }
    }
}

impl TelegramPoller {
    /// Remote-control polling loop.
    ///
    /// Concurrently processes:
    /// 1. Telegram updates (messages, callback queries) → bridge commands
    /// 2. Remote events from the agent → Telegram messages
    async fn run_remote(
        self: Arc<Self>,
        mut shutdown_rx: watch::Receiver<bool>,
        bridge: Arc<RemoteSessionBridge>,
    ) {
        let mut offset: i64 = 0;
        // Track per-chat state (for now, single chat — first DM that connects)
        let chat_state: Arc<tokio::sync::Mutex<Option<RemoteChatState>>> =
            Arc::new(tokio::sync::Mutex::new(None));

        loop {
            if self
                .remote_claim
                .as_ref()
                .is_some_and(|claim| !claim.is_current_owner())
            {
                warn!(
                    "Telegram remote: ownership transferred to another local session; stopping this poller"
                );
                break;
            }

            let mut event_rx = bridge.event_rx.lock().await;

            tokio::select! {
                _ = shutdown_rx.changed() => {
                    debug!("Telegram remote poller received shutdown signal");
                    break;
                }
                // Process remote events from agent
                event = event_rx.recv() => {
                    drop(event_rx); // release lock before processing
                    if let Some(event) = event {
                        let mut state = chat_state.lock().await;
                        if let Some(ref mut cs) = *state {
                            self.handle_remote_event(cs, &event).await;
                        }
                        // If no chat connected yet, events are dropped
                    } else {
                        debug!("Remote event channel closed");
                        break;
                    }
                }
                // Process Telegram updates
                result = self.api.get_updates(offset) => {
                    drop(event_rx); // release lock
                    match result {
                        Ok(updates) => {
                            for update in updates {
                                offset = update.update_id + 1;

                                // Handle callback queries (inline keyboard button presses)
                                if let Some(cb) = update.callback_query {
                                    let poller = Arc::clone(&self);
                                    let bridge = Arc::clone(&bridge);
                                    tokio::spawn(async move {
                                        poller.handle_callback_query(&cb, &bridge).await;
                                    });
                                    continue;
                                }

                                if let Some(msg) = update.message {
                                    let from = match &msg.from {
                                        Some(u) => u,
                                        None => continue,
                                    };
                                    let sender_id = from.id.to_string();
                                    let is_private = msg.chat.chat_type == "private";

                                    // DM access control — auto-pair first user in remote mode
                                    if is_private
                                        && self.dm_policy != DmPolicy::Open
                                        && !self.is_user_allowed(&sender_id).await
                                    {
                                        // Auto-approve the first DM user as owner
                                        let allowed = self.allowed_users.read().await;
                                        let has_users = !allowed.is_empty();
                                        drop(allowed);

                                        if !has_users {
                                            self.approve_user(&sender_id).await;
                                            let name = from.username.as_deref().unwrap_or(&from.first_name);
                                            info!("Telegram remote: auto-paired first user {} ({})", name, sender_id);
                                            let _ = self.api.send_message(SendMessageRequest {
                                                chat_id: msg.chat.id,
                                                text: format!("Auto-paired as owner. Welcome, {name}!"),
                                                parse_mode: None,
                                                reply_to_message_id: None,
                                            }).await;
                                            // Fall through to handle the message normally
                                        } else if self.dm_policy == DmPolicy::Pairing {
                                            self.send_pairing_challenge(&msg, &sender_id).await;
                                            continue;
                                        } else {
                                            continue;
                                        }
                                    }

                                    let text = msg.text.as_deref().unwrap_or("").trim().to_string();
                                    if text.is_empty() {
                                        continue;
                                    }

                                    // Register this chat for remote events
                                    {
                                        let mut state = chat_state.lock().await;
                                        if state.is_none() {
                                            *state = Some(RemoteChatState::new(msg.chat.id));
                                            info!("Telegram remote: connected chat_id={}", msg.chat.id);
                                        }
                                    }

                                    // Handle commands
                                    match text.as_str() {
                                        "/start" | "/help" => {
                                            let help = concat!(
                                                "OpenDev Remote Control\n\n",
                                                "Send a message to interact with the agent.\n\n",
                                                "Commands:\n",
                                                "/status  — Session status & context usage\n",
                                                "/cancel  — Cancel current operation\n",
                                                "/new     — Start a new session\n",
                                                "/resume  — Resume previous session\n",
                                                "/compact — Compact context window\n",
                                                "/cost    — Show session cost\n",
                                                "/help    — Show this help",
                                            );
                                            let _ = self.api.send_message(SendMessageRequest {
                                                chat_id: msg.chat.id,
                                                text: help.to_string(),
                                                parse_mode: None,
                                                reply_to_message_id: None,
                                            }).await;
                                        }
                                        "/status" => {
                                            let state = chat_state.lock().await;
                                            let status = if let Some(ref cs) = *state {
                                                if cs.agent_active {
                                                    format!("Agent is working (context: {:.0}%)", cs.context_usage)
                                                } else {
                                                    "Agent is idle. Send a message to start.".to_string()
                                                }
                                            } else {
                                                "Not connected to a session.".to_string()
                                            };
                                            let _ = self.api.send_message(SendMessageRequest {
                                                chat_id: msg.chat.id,
                                                text: status,
                                                parse_mode: None,
                                                reply_to_message_id: None,
                                            }).await;
                                        }
                                        "/cancel" => {
                                            bridge.send_command(RemoteCommand::Cancel);
                                            let _ = self.api.send_message(SendMessageRequest {
                                                chat_id: msg.chat.id,
                                                text: "Cancellation requested.".to_string(),
                                                parse_mode: None,
                                                reply_to_message_id: None,
                                            }).await;
                                        }
                                        "/new" => {
                                            bridge.send_command(RemoteCommand::NewSession);
                                            let _ = self.api.send_message(SendMessageRequest {
                                                chat_id: msg.chat.id,
                                                text: "Starting new session...".to_string(),
                                                parse_mode: None,
                                                reply_to_message_id: None,
                                            }).await;
                                        }
                                        "/resume" => {
                                            bridge.send_command(RemoteCommand::ResumeSession { session_id: None });
                                            let _ = self.api.send_message(SendMessageRequest {
                                                chat_id: msg.chat.id,
                                                text: "Resuming previous session...".to_string(),
                                                parse_mode: None,
                                                reply_to_message_id: None,
                                            }).await;
                                        }
                                        "/compact" => {
                                            bridge.send_command(RemoteCommand::Compact);
                                            let _ = self.api.send_message(SendMessageRequest {
                                                chat_id: msg.chat.id,
                                                text: "Compacting context...".to_string(),
                                                parse_mode: None,
                                                reply_to_message_id: None,
                                            }).await;
                                        }
                                        "/cost" => {
                                            bridge.send_command(RemoteCommand::Cost);
                                        }
                                        _ => {
                                            // Forward as chat message
                                            bridge.send_command(RemoteCommand::SendMessage(text));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if self
                                .remote_claim
                                .as_ref()
                                .is_some_and(|claim| !claim.is_current_owner())
                            {
                                info!("Telegram remote: ownership lost after polling error; shutting down");
                                break;
                            }
                            error!("Telegram remote getUpdates error: {}", e);
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        }
    }

    /// Handle a remote event by sending/editing Telegram messages.
    async fn handle_remote_event(&self, state: &mut RemoteChatState, event: &RemoteEvent) {
        let chat_id = state.chat_id;

        match event {
            RemoteEvent::AgentStarted => {
                state.agent_active = true;
                state.accumulated_text.clear();
                state.status_message_id = None;
                let _ = self.api.send_chat_action(chat_id, "typing").await;
            }
            RemoteEvent::AgentFinished => {
                state.agent_active = false;
                // Final flush of accumulated text
                if !state.accumulated_text.trim().is_empty() {
                    self.flush_accumulated_text(state).await;
                }
            }
            RemoteEvent::AgentInterrupted => {
                state.agent_active = false;
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: "Operation cancelled.".to_string(),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::AgentChunk(text) => {
                state.accumulated_text.push_str(text);
                // Throttle: edit at most once per second
                if state.last_edit.elapsed() >= Duration::from_millis(THROTTLE_MS) {
                    self.update_streaming_message(state).await;
                }
            }
            RemoteEvent::AgentError(err) => {
                state.agent_active = false;
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: format!("Error: {err}"),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::ToolStarted { tool_name, args } => {
                let args_summary = summarize_tool_args(tool_name, args);
                let text = format!("⚙ {tool_name}{args_summary}");
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text,
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::ToolFinished { tool_name, success } => {
                let icon = if *success { "✓" } else { "✗" };
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: format!("{icon} {tool_name} done"),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::ToolResult {
                tool_name,
                output,
                success,
            } => {
                // Show truncated result
                let icon = if *success { "✓" } else { "✗" };
                let truncated: String = output.chars().take(300).collect();
                let suffix = if output.len() > 300 { "…" } else { "" };
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: format!("{icon} {tool_name}:\n{truncated}{suffix}"),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::ToolApprovalNeeded {
                request_id,
                command,
                working_dir,
            } => {
                // Send approval request with inline keyboard
                let text = format!(
                    "🔐 Tool approval required\n\nCommand:\n{}\n\nWorking dir: {}",
                    command, working_dir
                );
                let keyboard = InlineKeyboardMarkup {
                    inline_keyboard: vec![vec![
                        InlineKeyboardButton {
                            text: "✅ Approve".to_string(),
                            callback_data: Some(format!("approve:{request_id}")),
                        },
                        InlineKeyboardButton {
                            text: "❌ Deny".to_string(),
                            callback_data: Some(format!("deny:{request_id}")),
                        },
                    ]],
                };
                let _ = self
                    .api
                    .send_message_with_markup(SendMessageWithMarkupRequest {
                        chat_id,
                        text,
                        parse_mode: None,
                        reply_to_message_id: None,
                        reply_markup: Some(keyboard),
                    })
                    .await;
            }
            RemoteEvent::AskUser {
                request_id,
                question,
                options,
            } => {
                if options.is_empty() {
                    // Free-form question — user replies with text
                    let text =
                        format!("❓ {question}\n\n(Reply with your answer, or /cancel to skip)");
                    let _ = self
                        .api
                        .send_message(SendMessageRequest {
                            chat_id,
                            text,
                            parse_mode: None,
                            reply_to_message_id: None,
                        })
                        .await;
                } else {
                    // Multiple choice — use inline keyboard
                    let buttons: Vec<InlineKeyboardButton> = options
                        .iter()
                        .map(|opt| InlineKeyboardButton {
                            text: opt.clone(),
                            callback_data: Some(format!("answer:{request_id}:{opt}")),
                        })
                        .collect();
                    // Arrange buttons in rows of 2
                    let rows: Vec<Vec<InlineKeyboardButton>> =
                        buttons.chunks(2).map(|c| c.to_vec()).collect();
                    let keyboard = InlineKeyboardMarkup {
                        inline_keyboard: rows,
                    };
                    let _ = self
                        .api
                        .send_message_with_markup(SendMessageWithMarkupRequest {
                            chat_id,
                            text: format!("❓ {question}"),
                            parse_mode: None,
                            reply_to_message_id: None,
                            reply_markup: Some(keyboard),
                        })
                        .await;
                }
            }
            RemoteEvent::SubagentStarted {
                subagent_name,
                task,
            } => {
                let truncated_task: String = task.chars().take(200).collect();
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: format!("🤖 Subagent '{subagent_name}' started: {truncated_task}"),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::SubagentFinished {
                subagent_name,
                success,
                result_summary,
            } => {
                let icon = if *success { "✅" } else { "❌" };
                let truncated: String = result_summary.chars().take(300).collect();
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: format!("{icon} Subagent '{subagent_name}' finished:\n{truncated}"),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::ContextUsage(pct) => {
                state.context_usage = *pct;
            }
            RemoteEvent::FileChangeSummary {
                files,
                additions,
                deletions,
            } => {
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: format!("📁 {files} file(s) changed: +{additions} -{deletions}"),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
            RemoteEvent::SessionTitleUpdated(title) => {
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id,
                        text: format!("📋 Session: {title}"),
                        parse_mode: None,
                        reply_to_message_id: None,
                    })
                    .await;
            }
        }
    }

    /// Update or create the streaming message with accumulated text.
    async fn update_streaming_message(&self, state: &mut RemoteChatState) {
        let display = format!("{}▍", state.accumulated_text.trim());

        if let Some(mid) = state.status_message_id {
            let _ = self
                .api
                .edit_message_text(EditMessageTextRequest {
                    chat_id: state.chat_id,
                    message_id: mid,
                    text: display,
                    parse_mode: None,
                    reply_markup: None,
                })
                .await;
        } else if state.accumulated_text.trim().len() >= MIN_INITIAL_CHARS {
            match self
                .api
                .send_message(SendMessageRequest {
                    chat_id: state.chat_id,
                    text: display,
                    parse_mode: None,
                    reply_to_message_id: None,
                })
                .await
            {
                Ok(m) => {
                    state.status_message_id = Some(m.message_id);
                }
                Err(e) => {
                    error!("Telegram remote: failed to send streaming message: {e}");
                }
            }
        }
        state.last_edit = tokio::time::Instant::now();
    }

    /// Flush accumulated text as the final message (convert to HTML).
    async fn flush_accumulated_text(&self, state: &mut RemoteChatState) {
        let trimmed = state.accumulated_text.trim();
        if trimmed.is_empty() {
            return;
        }

        let html = super::format::markdown_to_telegram_html(trimmed);
        let chunks = super::format::split_telegram_html(&html, 4000);

        for (i, chunk) in chunks.iter().enumerate() {
            if i == 0 {
                if let Some(mid) = state.status_message_id {
                    let edit_result = self
                        .api
                        .edit_message_text(EditMessageTextRequest {
                            chat_id: state.chat_id,
                            message_id: mid,
                            text: chunk.clone(),
                            parse_mode: Some("HTML".to_string()),
                            reply_markup: None,
                        })
                        .await;

                    if edit_result.is_err() {
                        // Fall back to plain text
                        let _ = self
                            .api
                            .edit_message_text(EditMessageTextRequest {
                                chat_id: state.chat_id,
                                message_id: mid,
                                text: trimmed.to_string(),
                                parse_mode: None,
                                reply_markup: None,
                            })
                            .await;
                        return;
                    }
                } else {
                    let send_result = self
                        .api
                        .send_message(SendMessageRequest {
                            chat_id: state.chat_id,
                            text: chunk.clone(),
                            parse_mode: Some("HTML".to_string()),
                            reply_to_message_id: None,
                        })
                        .await;

                    if send_result.is_err() {
                        let _ = self
                            .api
                            .send_message(SendMessageRequest {
                                chat_id: state.chat_id,
                                text: trimmed.to_string(),
                                parse_mode: None,
                                reply_to_message_id: None,
                            })
                            .await;
                        return;
                    }
                }
            } else {
                let _ = self
                    .api
                    .send_message(SendMessageRequest {
                        chat_id: state.chat_id,
                        text: chunk.clone(),
                        parse_mode: Some("HTML".to_string()),
                        reply_to_message_id: None,
                    })
                    .await;
            }
        }

        state.accumulated_text.clear();
        state.status_message_id = None;
    }

    /// Handle a Telegram callback query (inline keyboard button press).
    async fn handle_callback_query(
        &self,
        cb: &super::types::CallbackQuery,
        bridge: &RemoteSessionBridge,
    ) {
        let data = match &cb.data {
            Some(d) => d.as_str(),
            None => return,
        };

        if let Some(request_id) = data.strip_prefix("approve:") {
            let resolved = bridge
                .resolve_approval(
                    request_id,
                    ApprovalResponse::Approved {
                        command: String::new(), // use original command
                    },
                )
                .await;
            let ack = if resolved {
                "Approved ✅"
            } else {
                "Request expired"
            };
            let _ = self.api.answer_callback_query(&cb.id, Some(ack)).await;

            // Edit the message to show the decision
            if let Some(ref msg) = cb.message {
                let _ = self
                    .api
                    .edit_message_text(EditMessageTextRequest {
                        chat_id: msg.chat.id,
                        message_id: msg.message_id,
                        text: format!("{}\n\n✅ Approved", msg.text.as_deref().unwrap_or("")),
                        parse_mode: None,
                        reply_markup: None, // remove buttons
                    })
                    .await;
            }
        } else if let Some(request_id) = data.strip_prefix("deny:") {
            let resolved = bridge
                .resolve_approval(request_id, ApprovalResponse::Denied)
                .await;
            let ack = if resolved {
                "Denied ❌"
            } else {
                "Request expired"
            };
            let _ = self.api.answer_callback_query(&cb.id, Some(ack)).await;

            if let Some(ref msg) = cb.message {
                let _ = self
                    .api
                    .edit_message_text(EditMessageTextRequest {
                        chat_id: msg.chat.id,
                        message_id: msg.message_id,
                        text: format!("{}\n\n❌ Denied", msg.text.as_deref().unwrap_or("")),
                        parse_mode: None,
                        reply_markup: None,
                    })
                    .await;
            }
        } else if let Some(rest) = data.strip_prefix("answer:") {
            // Format: answer:<request_id>:<answer>
            if let Some((request_id, answer)) = rest.split_once(':') {
                let resolved = bridge
                    .resolve_question(request_id, answer.to_string())
                    .await;
                let ack = if resolved {
                    format!("Selected: {answer}")
                } else {
                    "Request expired".to_string()
                };
                let _ = self.api.answer_callback_query(&cb.id, Some(&ack)).await;

                if let Some(ref msg) = cb.message {
                    let _ = self
                        .api
                        .edit_message_text(EditMessageTextRequest {
                            chat_id: msg.chat.id,
                            message_id: msg.message_id,
                            text: format!("{}\n\n→ {answer}", msg.text.as_deref().unwrap_or("")),
                            parse_mode: None,
                            reply_markup: None,
                        })
                        .await;
                }
            }
        } else {
            let _ = self.api.answer_callback_query(&cb.id, None).await;
        }
    }
}

/// Summarize tool args for display (keep it brief).
fn summarize_tool_args(tool_name: &str, args: &HashMap<String, serde_json::Value>) -> String {
    match tool_name {
        "bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|cmd| {
                let truncated: String = cmd.chars().take(100).collect();
                let suffix = if cmd.len() > 100 { "…" } else { "" };
                format!(": `{truncated}{suffix}`")
            })
            .unwrap_or_default(),
        "read_file" | "write_file" | "edit_file" => args
            .get("file_path")
            .or_else(|| args.get("path"))
            .and_then(|v| v.as_str())
            .map(|p| format!(": {p}"))
            .unwrap_or_default(),
        "grep" | "glob" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|p| format!(": {p}"))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Strip HTML tags for plain-text fallback.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Unescape HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}
