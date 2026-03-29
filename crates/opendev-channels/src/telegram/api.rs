//! Low-level Telegram Bot API HTTP client.

use std::time::Duration;

use reqwest::Client;

use super::error::TelegramError;
use super::types::{
    EditMessageTextRequest, Message, SendMessageRequest, SendMessageWithMarkupRequest,
    TelegramResponse, Update, User,
};

/// HTTP client for the Telegram Bot API.
pub struct TelegramApi {
    token: String,
    client: Client,
    base_url: String,
}

impl TelegramApi {
    /// Create a new API client for the given bot token.
    pub fn new(token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(35))
            .build()
            .expect("failed to build reqwest client");
        let base_url = format!("https://api.telegram.org/bot{}", token);
        Self {
            token,
            client,
            base_url,
        }
    }

    /// Validate the bot token and retrieve bot info.
    pub async fn get_me(&self) -> Result<User, TelegramError> {
        let url = format!("{}/getMe", self.base_url);
        let resp: TelegramResponse<User> = self.client.get(&url).send().await?.json().await?;

        if resp.ok {
            resp.result
                .ok_or_else(|| TelegramError::Api("getMe returned ok but no result".to_string()))
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Long-poll for updates. Blocks server-side for up to 30 seconds.
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<Update>, TelegramError> {
        let url = format!("{}/getUpdates?offset={}&timeout=30", self.base_url, offset);
        let resp: TelegramResponse<Vec<Update>> =
            self.client.get(&url).send().await?.json().await?;

        if resp.ok {
            Ok(resp.result.unwrap_or_default())
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Send a text message.
    pub async fn send_message(&self, req: SendMessageRequest) -> Result<Message, TelegramError> {
        let url = format!("{}/sendMessage", self.base_url);
        let resp: TelegramResponse<Message> = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if resp.ok {
            resp.result.ok_or_else(|| {
                TelegramError::Api("sendMessage returned ok but no result".to_string())
            })
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Send a chat action (e.g., "typing") to indicate the bot is working.
    pub async fn send_chat_action(
        &self,
        chat_id: i64,
        action: &str,
    ) -> Result<bool, TelegramError> {
        let url = format!("{}/sendChatAction", self.base_url);
        let body = serde_json::json!({
            "chat_id": chat_id,
            "action": action,
        });
        let resp: TelegramResponse<bool> = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if resp.ok {
            Ok(resp.result.unwrap_or(true))
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Edit the text of an existing message.
    pub async fn edit_message_text(
        &self,
        req: EditMessageTextRequest,
    ) -> Result<Message, TelegramError> {
        let url = format!("{}/editMessageText", self.base_url);
        let resp: TelegramResponse<Message> = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if resp.ok {
            resp.result.ok_or_else(|| {
                TelegramError::Api("editMessageText returned ok but no result".to_string())
            })
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Send a text message with optional inline keyboard.
    pub async fn send_message_with_markup(
        &self,
        req: SendMessageWithMarkupRequest,
    ) -> Result<Message, TelegramError> {
        let url = format!("{}/sendMessage", self.base_url);
        let resp: TelegramResponse<Message> = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if resp.ok {
            resp.result.ok_or_else(|| {
                TelegramError::Api("sendMessage returned ok but no result".to_string())
            })
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Answer a callback query (acknowledge inline keyboard button press).
    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<bool, TelegramError> {
        let url = format!("{}/answerCallbackQuery", self.base_url);
        let mut body = serde_json::json!({
            "callback_query_id": callback_query_id,
        });
        if let Some(t) = text {
            body["text"] = serde_json::json!(t);
        }
        let resp: TelegramResponse<bool> = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if resp.ok {
            Ok(resp.result.unwrap_or(true))
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Set the bot's command menu (the list shown when user types "/").
    pub async fn set_my_commands(&self, commands: &[(&str, &str)]) -> Result<bool, TelegramError> {
        let url = format!("{}/setMyCommands", self.base_url);
        let cmds: Vec<serde_json::Value> = commands
            .iter()
            .map(|(cmd, desc)| {
                serde_json::json!({
                    "command": cmd,
                    "description": desc,
                })
            })
            .collect();
        let body = serde_json::json!({ "commands": cmds });
        let resp: TelegramResponse<bool> = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if resp.ok {
            Ok(resp.result.unwrap_or(true))
        } else {
            Err(TelegramError::Api(
                resp.description
                    .unwrap_or_else(|| "unknown error".to_string()),
            ))
        }
    }

    /// Get the bot token (for diagnostics/logging, not for display).
    pub fn token(&self) -> &str {
        &self.token
    }
}
