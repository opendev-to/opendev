//! Telegram Bot API types.
//!
//! Minimal serde structs matching the Telegram Bot API JSON schema.
//! Only includes what the MVP adapter needs (text messages).

use serde::{Deserialize, Serialize};

/// Generic Telegram API response wrapper.
#[derive(Debug, Deserialize)]
pub struct TelegramResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
}

/// Telegram user.
#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub username: Option<String>,
}

/// Telegram chat.
#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
    pub title: Option<String>,
    pub username: Option<String>,
}

/// Telegram message.
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<User>,
    pub chat: Chat,
    pub date: i64,
    pub text: Option<String>,
    pub reply_to_message: Option<Box<Message>>,
}

/// Telegram update from getUpdates.
#[derive(Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
    pub callback_query: Option<CallbackQuery>,
}

/// Request body for sendMessage.
#[derive(Debug, Serialize)]
pub struct SendMessageRequest {
    pub chat_id: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<i64>,
}

/// Request body for editMessageText.
#[derive(Debug, Serialize)]
pub struct EditMessageTextRequest {
    pub chat_id: i64,
    pub message_id: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

/// Inline keyboard markup for interactive buttons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

/// A single inline keyboard button.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
}

/// Telegram callback query (from inline keyboard button press).
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: User,
    pub message: Option<Message>,
    pub data: Option<String>,
}

/// Request body for sendMessage with optional inline keyboard.
#[derive(Debug, Serialize)]
pub struct SendMessageWithMarkupRequest {
    pub chat_id: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_update() {
        let json = r#"{
            "update_id": 123456,
            "message": {
                "message_id": 42,
                "from": {
                    "id": 789,
                    "is_bot": false,
                    "first_name": "Alice",
                    "username": "alice"
                },
                "chat": {
                    "id": 789,
                    "type": "private",
                    "username": "alice"
                },
                "date": 1700000000,
                "text": "Hello bot"
            }
        }"#;

        let update: Update = serde_json::from_str(json).unwrap();
        assert_eq!(update.update_id, 123456);
        let msg = update.message.unwrap();
        assert_eq!(msg.message_id, 42);
        assert_eq!(msg.text.unwrap(), "Hello bot");
        assert_eq!(msg.chat.chat_type, "private");
        assert_eq!(msg.from.unwrap().username.unwrap(), "alice");
    }

    #[test]
    fn test_deserialize_group_message() {
        let json = r#"{
            "update_id": 100,
            "message": {
                "message_id": 1,
                "from": {
                    "id": 111,
                    "is_bot": false,
                    "first_name": "Bob"
                },
                "chat": {
                    "id": -100123,
                    "type": "supergroup",
                    "title": "Dev Chat"
                },
                "date": 1700000000,
                "text": "@mybot help me"
            }
        }"#;

        let update: Update = serde_json::from_str(json).unwrap();
        let msg = update.message.unwrap();
        assert_eq!(msg.chat.chat_type, "supergroup");
        assert_eq!(msg.chat.title.unwrap(), "Dev Chat");
        assert!(msg.chat.id < 0); // group chats have negative IDs
    }

    #[test]
    fn test_serialize_send_message_request() {
        let req = SendMessageRequest {
            chat_id: 789,
            text: "Hello!".to_string(),
            parse_mode: Some("MarkdownV2".to_string()),
            reply_to_message_id: Some(42),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["chat_id"], 789);
        assert_eq!(json["text"], "Hello!");
        assert_eq!(json["parse_mode"], "MarkdownV2");
        assert_eq!(json["reply_to_message_id"], 42);
    }

    #[test]
    fn test_serialize_send_message_request_minimal() {
        let req = SendMessageRequest {
            chat_id: 789,
            text: "Hello!".to_string(),
            parse_mode: None,
            reply_to_message_id: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["chat_id"], 789);
        assert!(json.get("parse_mode").is_none());
        assert!(json.get("reply_to_message_id").is_none());
    }
}
