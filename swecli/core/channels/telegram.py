"""Telegram channel adapter (skeleton).

This adapter integrates with Telegram Bot API to receive and send messages.
Full implementation requires python-telegram-bot library.

TODO: Implement when ready to deploy to Telegram
- Install: uv pip install python-telegram-bot
- Set up bot token from @BotFather
- Implement message handlers
- Handle media/file attachments
- Support group chats and topics
"""

from typing import Any, Optional

from swecli.core.channels.base import ChannelAdapter, InboundMessage, OutboundMessage


class TelegramAdapter(ChannelAdapter):
    """Telegram Bot API adapter (not yet implemented).

    Example configuration:
        adapter = TelegramAdapter(
            bot_token=os.environ["TELEGRAM_BOT_TOKEN"],
            allowed_users=["@alice", "@bob"],  # Optional whitelist
        )
        await adapter.start()

    Features to implement:
    - Long polling for messages (or webhooks)
    - Convert Telegram Message → InboundMessage
    - Convert OutboundMessage → Telegram sendMessage
    - Handle /start command for workspace selection
    - Support media uploads/downloads
    - Thread support (Telegram topics/forum)
    """

    def __init__(
        self,
        bot_token: str,
        allowed_users: Optional[list[str]] = None,
        webhook_url: Optional[str] = None,
    ):
        """Initialize Telegram adapter.

        Args:
            bot_token: Bot token from @BotFather
            allowed_users: Optional list of allowed Telegram usernames
            webhook_url: Optional webhook URL (alternative to polling)
        """
        self.channel_name = "telegram"
        self.bot_token = bot_token
        self.allowed_users = allowed_users
        self.webhook_url = webhook_url

    async def start(self) -> None:
        """Start Telegram bot (not implemented)."""
        raise NotImplementedError(
            "TelegramAdapter not yet implemented. "
            "See swecli/core/channels/telegram.py for TODO items."
        )

    async def send(self, delivery_context: dict[str, Any], message: OutboundMessage) -> None:
        """Send message to Telegram user (not implemented)."""
        raise NotImplementedError(
            "TelegramAdapter not yet implemented. "
            "See swecli/core/channels/telegram.py for TODO items."
        )

    async def stop(self) -> None:
        """Stop Telegram bot (not implemented)."""
        pass  # No-op for now
