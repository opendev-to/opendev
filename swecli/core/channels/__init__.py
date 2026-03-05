"""Multi-channel messaging abstractions for OpenDev.

Provides a unified interface for different messaging channels (Telegram, WhatsApp,
Web UI, CLI) to interact with the agent.

Example:
    # Create a Telegram adapter
    adapter = TelegramAdapter(bot_token="...")
    await adapter.start()

    # Incoming messages are routed to sessions
    # Outgoing responses are sent back via the adapter
"""

from swecli.core.channels.base import (
    ChannelAdapter,
    InboundMessage,
    OutboundMessage,
    MessageAttachment,
    AttachmentType,
)

__all__ = [
    "ChannelAdapter",
    "InboundMessage",
    "OutboundMessage",
    "MessageAttachment",
    "AttachmentType",
]
