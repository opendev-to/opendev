//! Message tool — send messages to external channels via webhooks.
//!
//! Supports Slack, Discord, and generic webhook channels.
//! Channel configuration is loaded from `~/.opendev/settings.json`
//! or `.opendev/settings.json`.

use std::collections::HashMap;
use std::path::PathBuf;

use opendev_tools_core::{BaseTool, ToolContext, ToolDisplayMeta, ToolResult};

/// Tool for sending messages to configured channels.
#[derive(Debug)]
pub struct MessageTool;

#[async_trait::async_trait]
impl BaseTool for MessageTool {
    fn name(&self) -> &str {
        "message"
    }

    fn description(&self) -> &str {
        "Send a message to a configured channel (Slack, Discord, or generic webhook)."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "description": "Channel type: 'slack', 'discord', or 'webhook'"
                },
                "target": {
                    "type": "string",
                    "description": "Webhook URL (overrides configured default)"
                },
                "message": {
                    "type": "string",
                    "description": "Message content to send"
                },
                "format": {
                    "type": "string",
                    "description": "Message format: 'text' (default) or 'markdown'",
                    "enum": ["text", "markdown"]
                }
            },
            "required": ["channel", "message"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let channel = match args.get("channel").and_then(|v| v.as_str()) {
            Some(c) if !c.is_empty() => c,
            _ => return ToolResult::fail("channel is required"),
        };

        let message = match args.get("message").and_then(|v| v.as_str()) {
            Some(m) if !m.is_empty() => m,
            _ => return ToolResult::fail("message is required"),
        };

        let target = args.get("target").and_then(|v| v.as_str());
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("text");

        // Load channel config
        let channel_config = load_channel_config();
        let config_for_channel = channel_config.get(channel).and_then(|v| v.as_object());

        // Determine webhook URL
        let webhook_url = target.map(|t| t.to_string()).or_else(|| {
            config_for_channel
                .and_then(|c| c.get("webhook_url"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

        let webhook_url = match webhook_url {
            Some(url) if !url.is_empty() => url,
            _ => {
                return ToolResult::fail(format!(
                    "No webhook URL configured for channel '{channel}'. \
                     Set it in ~/.opendev/settings.json under channels.{channel}.webhook_url \
                     or pass it as the 'target' parameter."
                ));
            }
        };

        if !webhook_url.starts_with("http://") && !webhook_url.starts_with("https://") {
            return ToolResult::fail("Webhook URL must start with http:// or https://");
        }

        // Build payload based on channel type
        let payload = build_payload(channel, message, format);

        // Send the webhook
        let client = match reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(10))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        match client
            .post(&webhook_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status().as_u16();
                if (200..300).contains(&status) {
                    ToolResult::ok(format!("Message sent to {channel} (status {status})"))
                } else {
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "unknown error".to_string());
                    ToolResult::fail(format!("Webhook returned status {status}: {body}"))
                }
            }
            Err(e) => ToolResult::fail(format!("Failed to send message: {e}")),
        }
    }

    fn display_meta(&self) -> Option<ToolDisplayMeta> {
        Some(ToolDisplayMeta {
            verb: "Message",
            label: "channel",
            category: "Other",
            primary_arg_keys: &["channel", "message"],
        })
    }
}

/// Build the webhook payload for a specific channel type.
fn build_payload(channel: &str, message: &str, format: &str) -> serde_json::Value {
    match channel {
        "slack" => {
            if format == "markdown" {
                serde_json::json!({
                    "blocks": [{
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": message
                        }
                    }]
                })
            } else {
                serde_json::json!({ "text": message })
            }
        }
        "discord" => {
            serde_json::json!({ "content": message })
        }
        _ => {
            // Generic webhook
            serde_json::json!({
                "text": message,
                "format": format
            })
        }
    }
}

/// Load channel configuration from settings files.
fn load_channel_config() -> HashMap<String, serde_json::Value> {
    let mut channels = HashMap::new();

    let config_paths = [
        Some(opendev_config::Paths::default().global_settings()),
        Some(PathBuf::from(".opendev").join("settings.json")),
    ];

    for path in config_paths.iter().flatten() {
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(path)
            && let Ok(data) = serde_json::from_str::<serde_json::Value>(&content)
            && let Some(ch) = data.get("channels").and_then(|v| v.as_object())
        {
            for (key, value) in ch {
                channels.insert(key.clone(), value.clone());
            }
        }
    }

    channels
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
