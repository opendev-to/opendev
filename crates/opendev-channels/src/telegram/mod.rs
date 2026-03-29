//! Telegram bot adapter for OpenDev channels.
//!
//! Provides a `ChannelAdapter` implementation that connects to the Telegram
//! Bot API via long polling, routing messages through the `MessageRouter`.

pub mod adapter;
pub mod api;
pub mod error;
pub mod format;
pub mod polling;
pub mod remote;
mod remote_claim;
pub mod types;

pub use adapter::TelegramAdapter;
pub use error::TelegramError;
pub use polling::TelegramPoller;
pub use remote::RemoteSessionBridge;

use std::collections::HashSet;
use std::sync::Arc;

use crate::router::MessageRouter;
use tokio::sync::{RwLock, watch};
use tracing::info;

use api::TelegramApi;
use remote_claim::RemoteSessionClaim;

/// DM access policy (mirrors `opendev_models::DmPolicy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmPolicy {
    Open,
    Pairing,
    Allowlist,
}

/// Token configuration for the Telegram bot.
pub struct TelegramConfig {
    pub bot_token: String,
    pub enabled: bool,
    pub group_mention_only: bool,
    pub dm_policy: DmPolicy,
    pub allowed_users: Vec<String>,
}

/// Resolve the bot token from config or environment.
///
/// Priority: explicit config > `TELEGRAM_BOT_TOKEN` env var.
pub fn resolve_token(config: Option<&TelegramConfig>) -> Result<String, TelegramError> {
    if let Some(cfg) = config
        && !cfg.bot_token.is_empty()
    {
        return Ok(cfg.bot_token.clone());
    }

    std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| TelegramError::InvalidToken)
}

/// Build and start Telegram in remote-control mode.
///
/// Instead of creating isolated per-chat agent sessions, the bot attaches
/// to the running TUI session via a `RemoteSessionBridge`. Telegram users
/// can observe agent activity, approve/deny tool calls, send messages, and
/// cancel operations.
///
/// Returns `(adapter, shutdown_handle, bridge)`.
pub async fn start_telegram_remote(
    config: Option<&TelegramConfig>,
) -> Result<
    (
        Arc<TelegramAdapter>,
        watch::Sender<bool>,
        Arc<RemoteSessionBridge>,
        remote::RemoteEventSender,
        remote::RemoteCommandReceiver,
    ),
    TelegramError,
> {
    let token = resolve_token(config)?;
    let dm_policy = config.map(|c| c.dm_policy).unwrap_or(DmPolicy::Pairing);
    let allowed_users: HashSet<String> = config
        .map(|c| c.allowed_users.iter().cloned().collect())
        .unwrap_or_default();
    let remote_claim = Arc::new(RemoteSessionClaim::claim(&token)?);

    let api = Arc::new(TelegramApi::new(token));

    // Validate token
    let me = api.get_me().await?;
    let bot_username = me.username.unwrap_or_default();
    let bot_id = me.id;

    info!(
        "Telegram remote-control bot authenticated as @{}",
        bot_username
    );

    // Register slash command menu
    let _ = api
        .set_my_commands(&[
            ("status", "Show session status"),
            ("cancel", "Cancel current operation"),
            ("new", "Start a new session"),
            ("resume", "Resume previous session"),
            ("compact", "Compact context window"),
            ("cost", "Show session cost"),
            ("help", "Show available commands"),
        ])
        .await;

    let adapter = Arc::new(TelegramAdapter {
        api: api.clone(),
        bot_username: bot_username.clone(),
    });

    // Create the remote session bridge
    let (bridge, event_tx, command_rx) = RemoteSessionBridge::new();

    // Start polling in remote mode
    let poller = Arc::new(TelegramPoller {
        api,
        router: Arc::new(MessageRouter::new()), // unused in remote mode
        bot_username,
        bot_id,
        group_mention_only: false, // remote mode handles all DMs
        dm_policy,
        allowed_users: Arc::new(RwLock::new(allowed_users)),
        remote_claim: Some(remote_claim),
    });
    let shutdown = poller.spawn_remote(Arc::clone(&bridge));

    Ok((adapter, shutdown, bridge, event_tx, command_rx))
}

/// Build and start the Telegram adapter and polling loop.
///
/// Validates the bot token via `getMe`, registers the adapter with the router,
/// and spawns a background polling task.
///
/// Returns the adapter and a shutdown handle. Drop the handle to stop polling.
pub async fn start_telegram(
    config: Option<&TelegramConfig>,
    router: Arc<MessageRouter>,
) -> Result<(Arc<TelegramAdapter>, watch::Sender<bool>), TelegramError> {
    let token = resolve_token(config)?;
    let group_mention_only = config.map(|c| c.group_mention_only).unwrap_or(true);
    let dm_policy = config.map(|c| c.dm_policy).unwrap_or(DmPolicy::Pairing);
    let allowed_users: HashSet<String> = config
        .map(|c| c.allowed_users.iter().cloned().collect())
        .unwrap_or_default();

    let api = Arc::new(TelegramApi::new(token));

    // Validate token
    let me = api.get_me().await?;
    let bot_username = me.username.unwrap_or_default();
    let bot_id = me.id;

    info!("Telegram bot authenticated as @{}", bot_username);

    let adapter = Arc::new(TelegramAdapter {
        api: api.clone(),
        bot_username: bot_username.clone(),
    });

    // Register with router
    router.register_adapter(adapter.clone()).await;

    // Start polling
    let poller = Arc::new(TelegramPoller {
        api,
        router,
        bot_username,
        bot_id,
        group_mention_only,
        dm_policy,
        allowed_users: Arc::new(RwLock::new(allowed_users)),
        remote_claim: None,
    });
    let shutdown = poller.spawn();

    Ok((adapter, shutdown))
}
