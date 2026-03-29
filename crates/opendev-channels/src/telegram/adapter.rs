//! Telegram channel adapter implementing `ChannelAdapter`.

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{ChannelError, ChannelResult};
use crate::router::{ChannelAdapter, DeliveryContext, OutboundMessage};

use super::api::TelegramApi;
use super::types::SendMessageRequest;

/// Maximum message length for Telegram messages.
const TELEGRAM_MAX_MESSAGE_LEN: usize = 4096;

/// Telegram channel adapter that sends messages via the Telegram Bot API.
pub struct TelegramAdapter {
    pub(super) api: Arc<TelegramApi>,
    pub(super) bot_username: String,
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn channel_name(&self) -> &str {
        "telegram"
    }

    async fn send(
        &self,
        delivery_context: &DeliveryContext,
        message: OutboundMessage,
    ) -> ChannelResult<()> {
        let chat_id = delivery_context
            .get("chat_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ChannelError::DeliveryFailed {
                channel: "telegram".into(),
                message: "missing chat_id in delivery context".into(),
            })?;

        let reply_to = message
            .reply_to_message_id
            .as_ref()
            .and_then(|id| id.parse::<i64>().ok());

        // Only use parse_mode if explicitly set by the caller.
        // Do NOT default to MarkdownV2 — LLM responses contain unescaped
        // special characters that Telegram's strict parser rejects.
        let parse_mode = message.parse_mode;

        let chunks = split_message(&message.text, TELEGRAM_MAX_MESSAGE_LEN);

        for (i, chunk) in chunks.iter().enumerate() {
            let req = SendMessageRequest {
                chat_id,
                text: chunk.clone(),
                parse_mode: parse_mode.clone(),
                reply_to_message_id: if i == 0 { reply_to } else { None },
            };
            self.api
                .send_message(req)
                .await
                .map_err(|e| ChannelError::DeliveryFailed {
                    channel: "telegram".into(),
                    message: e.to_string(),
                })?;
        }

        Ok(())
    }
}

impl TelegramAdapter {
    /// Get the bot username.
    pub fn bot_username(&self) -> &str {
        &self.bot_username
    }
}

/// Split text into chunks respecting Telegram's max message length.
/// Tries to split on double-newline, then single newline, then hard split.
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        let search_region = &remaining[..max_len];
        let split_pos = search_region
            .rfind("\n\n")
            .or_else(|| search_region.rfind('\n'))
            .unwrap_or(max_len);

        // Avoid zero-length splits
        let split_pos = if split_pos == 0 { max_len } else { split_pos };

        chunks.push(remaining[..split_pos].to_string());
        remaining = remaining[split_pos..].trim_start_matches('\n');
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_short_message() {
        let chunks = split_message("hello", 4096);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_split_on_double_newline() {
        let text = format!("{}\n\n{}", "a".repeat(100), "b".repeat(100));
        let chunks = split_message(&text, 150);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "a".repeat(100));
        assert_eq!(chunks[1], "b".repeat(100));
    }

    #[test]
    fn test_split_on_single_newline() {
        let text = format!("{}\n{}", "a".repeat(100), "b".repeat(100));
        let chunks = split_message(&text, 150);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "a".repeat(100));
        assert_eq!(chunks[1], "b".repeat(100));
    }

    #[test]
    fn test_hard_split() {
        let text = "a".repeat(200);
        let chunks = split_message(&text, 100);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 100);
    }

    #[test]
    fn test_split_empty() {
        let chunks = split_message("", 4096);
        assert_eq!(chunks, vec![""]);
    }

    #[test]
    fn test_split_exact_boundary() {
        let text = "a".repeat(4096);
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 1);
    }
}
