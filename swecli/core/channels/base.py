"""Base abstractions for multi-channel messaging."""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any, Optional


class AttachmentType(str, Enum):
    """Types of message attachments."""

    IMAGE = "image"
    FILE = "file"
    AUDIO = "audio"
    VIDEO = "video"
    DOCUMENT = "document"


@dataclass
class MessageAttachment:
    """Represents a file attachment in a message.

    Attributes:
        type: Type of attachment
        url: URL to download the attachment (if applicable)
        file_path: Local file path (if already downloaded)
        filename: Original filename
        mime_type: MIME type of the file
        size_bytes: File size in bytes
        metadata: Channel-specific metadata
    """

    type: AttachmentType
    filename: str
    url: Optional[str] = None
    file_path: Optional[str] = None
    mime_type: Optional[str] = None
    size_bytes: Optional[int] = None
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class InboundMessage:
    """Message received from a channel (user → agent).

    This is the unified format for all incoming messages, regardless of channel.
    Channel-specific details are stored in the `raw` field.

    Attributes:
        channel: Channel name ("telegram", "whatsapp", "web", "cli")
        user_id: Channel-specific user identifier (@user, +phone, session_id, etc.)
        text: Message text content
        timestamp: When the message was sent
        thread_id: Optional thread/topic ID for threaded channels
        chat_type: Type of chat ("direct", "group")
        attachments: List of file attachments
        reply_to_message_id: ID of message being replied to (if any)
        metadata: Channel-specific metadata (chat ID, language, etc.)
        raw: Original raw message payload from the channel
    """

    channel: str
    user_id: str
    text: str
    timestamp: datetime = field(default_factory=datetime.now)
    thread_id: Optional[str] = None
    chat_type: str = "direct"
    attachments: list[MessageAttachment] = field(default_factory=list)
    reply_to_message_id: Optional[str] = None
    metadata: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] = field(default_factory=dict)


@dataclass
class OutboundMessage:
    """Message to send to a channel (agent → user).

    This is the unified format for all outgoing messages. The channel adapter
    is responsible for converting this to the channel's native format.

    Attributes:
        text: Message text content
        thread_id: Optional thread/topic ID to reply in (for threaded channels)
        reply_to_message_id: ID of message to reply to (if applicable)
        attachments: List of file attachments to send
        parse_mode: Text formatting mode ("markdown", "html", "plain")
        disable_preview: Disable link previews in the message
        metadata: Channel-specific metadata (buttons, keyboards, etc.)
    """

    text: str
    thread_id: Optional[str] = None
    reply_to_message_id: Optional[str] = None
    attachments: list[MessageAttachment] = field(default_factory=list)
    parse_mode: str = "markdown"
    disable_preview: bool = False
    metadata: dict[str, Any] = field(default_factory=dict)


class ChannelAdapter(ABC):
    """Abstract base class for channel adapters.

    A channel adapter handles communication with a specific messaging platform
    (Telegram, WhatsApp, Web UI, CLI). It converts between the platform's
    native message format and OpenDev's unified InboundMessage/OutboundMessage format.

    The adapter is responsible for:
    - Receiving messages from the channel
    - Converting them to InboundMessage format
    - Forwarding to the message router
    - Receiving OutboundMessage from the agent
    - Sending to the correct user/thread on the channel

    Subclasses must implement:
    - start(): Initialize and start listening for messages
    - send(): Send an outbound message to a user
    - stop(): Gracefully shutdown the adapter
    """

    channel_name: str  # "telegram", "whatsapp", "web", "cli"

    @abstractmethod
    async def start(self) -> None:
        """Start the channel adapter and begin listening for messages.

        This should:
        1. Initialize the connection to the messaging platform
        2. Set up message handlers/listeners
        3. Start the event loop (if needed)

        Raises:
            Exception: If the adapter fails to start
        """

    @abstractmethod
    async def send(self, delivery_context: dict[str, Any], message: OutboundMessage) -> None:
        """Send an outbound message to a user.

        Args:
            delivery_context: Where to send the message (user_id, chat_id, etc.)
                             Format is channel-specific but typically includes:
                             - user_id: Channel-specific user identifier
                             - chat_id: Chat/conversation ID (if different from user_id)
                             - thread_id: Thread ID (for threaded channels)
            message: The message to send

        Raises:
            Exception: If the message fails to send
        """

    @abstractmethod
    async def stop(self) -> None:
        """Gracefully stop the channel adapter.

        This should:
        1. Stop listening for new messages
        2. Close connections to the messaging platform
        3. Clean up resources

        Should not raise exceptions (swallow and log errors).
        """

    def __repr__(self) -> str:
        """String representation of the adapter."""
        return f"<{self.__class__.__name__} channel={self.channel_name}>"
