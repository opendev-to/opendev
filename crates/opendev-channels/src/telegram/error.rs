//! Telegram adapter error types.

#[derive(Debug, thiserror::Error)]
pub enum TelegramError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Telegram API error: {0}")]
    Api(String),

    #[error(
        "TELEGRAM_BOT_TOKEN not set — configure via `opendev channel add telegram` or settings.json"
    )]
    InvalidToken,
}
