//! Remote control integration for the TUI runner (Telegram remote sessions).
//!
//! Extracts the remote event callbacks, command listener, and approval bridge
//! from the main TUI runner to keep the core loop focused on local TUI concerns.

use std::sync::Arc;

use tokio::sync::mpsc;

use opendev_channels::telegram::remote::{
    ApprovalResponse, RemoteCommand, RemoteCommandReceiver, RemoteEvent, RemoteEventSender,
};
use opendev_channels::telegram::RemoteSessionBridge;
use opendev_runtime::ToolApprovalDecision;
use opendev_tui::AppEvent;

use crate::runtime::AgentRuntime;

/// Spawn the remote command listener that forwards Telegram commands to the TUI.
pub fn spawn_command_listener(
    mut remote_rx: RemoteCommandReceiver,
    user_tx: mpsc::UnboundedSender<String>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) {
    tokio::spawn(async move {
        while let Some(cmd) = remote_rx.recv().await {
            match cmd {
                RemoteCommand::SendMessage(text) => {
                    let _ = user_tx.send(text);
                }
                RemoteCommand::Cancel => {
                    let _ = event_tx.send(AppEvent::Interrupt);
                }
                RemoteCommand::Compact => {
                    let _ = user_tx.send("\x00__COMPACT__".to_string());
                }
                RemoteCommand::NewSession | RemoteCommand::ResumeSession { .. } => {
                    // Not supported in TUI+remote mode (TUI owns the session)
                }
                RemoteCommand::Cost => {}
                // Approval and question resolution is handled directly by the bridge
                RemoteCommand::ApproveToolCall { .. }
                | RemoteCommand::DenyToolCall { .. }
                | RemoteCommand::AnswerQuestion { .. } => {}
            }
        }
    });
}

/// Spawn the approval bridge that races TUI and Telegram approval responses.
///
/// Intercepts the tool approval channel: when a request comes in, it forwards
/// to both TUI and Telegram. Whichever responds first wins.
pub fn spawn_approval_bridge(
    bridge: Arc<RemoteSessionBridge>,
    rtx: RemoteEventSender,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    runtime: &mut AgentRuntime,
) {
    let Some(receivers) = runtime.channel_receivers.as_mut() else {
        return;
    };

    let mut original_approval_rx = std::mem::replace(&mut receivers.tool_approval_rx, {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        rx
    });

    tokio::spawn(async move {
        while let Some(req) = original_approval_rx.recv().await {
            let request_id = bridge.next_id();

            // Send to Telegram
            let _ = rtx.send(RemoteEvent::ToolApprovalNeeded {
                request_id: request_id.clone(),
                command: req.command.clone(),
                working_dir: req.working_dir.clone(),
            });

            // Register pending approval in bridge
            let remote_rx = bridge.register_approval(&request_id).await;

            // Also send to TUI
            let (tui_resp_tx, tui_resp_rx) = tokio::sync::oneshot::channel();
            let _ = event_tx.send(AppEvent::ToolApprovalRequested {
                command: req.command.clone(),
                working_dir: req.working_dir.clone(),
                response_tx: tui_resp_tx,
            });

            // Race: first response wins (TUI or Telegram)
            let original_command = req.command;
            tokio::select! {
                tui_result = tui_resp_rx => {
                    if let Ok(decision) = tui_result {
                        let _ = req.response_tx.send(decision);
                    }
                }
                remote_result = remote_rx => {
                    if let Ok(response) = remote_result {
                        match response {
                            ApprovalResponse::Approved { command } => {
                                let cmd = if command.is_empty() {
                                    original_command
                                } else {
                                    command
                                };
                                let _ = req.response_tx.send(
                                    ToolApprovalDecision {
                                        approved: true,
                                        choice: "yes".to_string(),
                                        command: cmd,
                                    },
                                );
                            }
                            ApprovalResponse::Denied => {
                                let _ = req.response_tx.send(
                                    ToolApprovalDecision {
                                        approved: false,
                                        choice: "no".to_string(),
                                        command: original_command,
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }
    });
}
