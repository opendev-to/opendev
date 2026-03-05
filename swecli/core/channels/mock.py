"""Mock channel adapter for testing."""

import asyncio
from typing import Any, Callable, Optional

from swecli.core.channels.base import ChannelAdapter, InboundMessage, OutboundMessage


class MockChannelAdapter(ChannelAdapter):
    """Mock channel adapter for testing multi-channel functionality.

    This adapter simulates a messaging channel by storing messages in memory
    and providing methods to inject inbound messages and inspect outbound messages.

    Example:
        adapter = MockChannelAdapter("test-channel")
        await adapter.start()

        # Simulate incoming message
        await adapter.simulate_inbound(
            user_id="test-user",
            text="Hello, agent!"
        )

        # Check outbound messages
        assert len(adapter.outbound_messages) == 1
        assert adapter.outbound_messages[0].text == "Hello! How can I help?"
    """

    def __init__(
        self,
        channel_name: str = "mock",
        on_inbound: Optional[Callable[[InboundMessage], None]] = None,
    ):
        """Initialize mock adapter.

        Args:
            channel_name: Name of the mock channel
            on_inbound: Optional callback for when inbound messages are received
        """
        self.channel_name = channel_name
        self._on_inbound = on_inbound
        self._running = False

        # Storage for testing
        self.inbound_messages: list[InboundMessage] = []
        self.outbound_messages: list[OutboundMessage] = []
        self.delivery_contexts: list[dict[str, Any]] = []

    async def start(self) -> None:
        """Start the mock adapter (no-op for testing)."""
        self._running = True

    async def send(self, delivery_context: dict[str, Any], message: OutboundMessage) -> None:
        """Store outbound message for testing inspection.

        Args:
            delivery_context: Where to send (stored for testing)
            message: Message to send (stored for testing)
        """
        if not self._running:
            raise RuntimeError("Adapter not started")

        self.outbound_messages.append(message)
        self.delivery_contexts.append(delivery_context)

    async def stop(self) -> None:
        """Stop the mock adapter (no-op for testing)."""
        self._running = False

    async def simulate_inbound(
        self,
        user_id: str,
        text: str,
        thread_id: Optional[str] = None,
        chat_type: str = "direct",
        **kwargs: Any,
    ) -> InboundMessage:
        """Simulate an inbound message from a user.

        This is a test helper to inject messages into the system.

        Args:
            user_id: User identifier
            text: Message text
            thread_id: Optional thread ID
            chat_type: Type of chat
            **kwargs: Additional InboundMessage fields

        Returns:
            The created InboundMessage
        """
        if not self._running:
            raise RuntimeError("Adapter not started")

        message = InboundMessage(
            channel=self.channel_name,
            user_id=user_id,
            text=text,
            thread_id=thread_id,
            chat_type=chat_type,
            **kwargs,
        )

        self.inbound_messages.append(message)

        # Call callback if provided
        if self._on_inbound:
            await asyncio.coroutine(self._on_inbound)(message)

        return message

    def clear(self) -> None:
        """Clear all stored messages (useful between tests)."""
        self.inbound_messages.clear()
        self.outbound_messages.clear()
        self.delivery_contexts.clear()

    def get_last_outbound(self) -> Optional[OutboundMessage]:
        """Get the last outbound message sent (test helper)."""
        return self.outbound_messages[-1] if self.outbound_messages else None

    def get_outbound_to_user(self, user_id: str) -> list[OutboundMessage]:
        """Get all outbound messages sent to a specific user (test helper)."""
        results = []
        for msg, ctx in zip(self.outbound_messages, self.delivery_contexts):
            if ctx.get("user_id") == user_id:
                results.append(msg)
        return results
