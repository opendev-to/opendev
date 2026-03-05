"""Tests for multi-channel abstractions."""

import pytest

from swecli.core.channels.base import (
    AttachmentType,
    InboundMessage,
    MessageAttachment,
    OutboundMessage,
)
from swecli.core.channels.mock import MockChannelAdapter


class TestMessageModels:
    """Test InboundMessage and OutboundMessage models."""

    def test_inbound_message_basic(self):
        """Test creating a basic inbound message."""
        msg = InboundMessage(
            channel="telegram",
            user_id="@alice",
            text="Hello, agent!",
        )

        assert msg.channel == "telegram"
        assert msg.user_id == "@alice"
        assert msg.text == "Hello, agent!"
        assert msg.chat_type == "direct"
        assert msg.thread_id is None
        assert len(msg.attachments) == 0

    def test_inbound_message_with_thread(self):
        """Test inbound message with thread ID."""
        msg = InboundMessage(
            channel="telegram",
            user_id="@alice",
            text="Question in thread",
            thread_id="topic_123",
            chat_type="group",
        )

        assert msg.thread_id == "topic_123"
        assert msg.chat_type == "group"

    def test_inbound_message_with_attachments(self):
        """Test inbound message with file attachments."""
        attachment = MessageAttachment(
            type=AttachmentType.IMAGE,
            filename="screenshot.png",
            url="https://example.com/image.png",
            mime_type="image/png",
            size_bytes=1024,
        )

        msg = InboundMessage(
            channel="whatsapp",
            user_id="+1234567890",
            text="Check this out",
            attachments=[attachment],
        )

        assert len(msg.attachments) == 1
        assert msg.attachments[0].type == AttachmentType.IMAGE
        assert msg.attachments[0].filename == "screenshot.png"

    def test_outbound_message_basic(self):
        """Test creating a basic outbound message."""
        msg = OutboundMessage(text="Hello, user!")

        assert msg.text == "Hello, user!"
        assert msg.parse_mode == "markdown"
        assert msg.disable_preview is False
        assert msg.thread_id is None

    def test_outbound_message_with_options(self):
        """Test outbound message with various options."""
        msg = OutboundMessage(
            text="Check this link: https://example.com",
            parse_mode="html",
            disable_preview=True,
            thread_id="topic_456",
        )

        assert msg.parse_mode == "html"
        assert msg.disable_preview is True
        assert msg.thread_id == "topic_456"


class TestMockChannelAdapter:
    """Test MockChannelAdapter functionality."""

    @pytest.mark.asyncio
    async def test_start_stop(self):
        """Test starting and stopping the adapter."""
        adapter = MockChannelAdapter("test")

        assert not adapter._running

        await adapter.start()
        assert adapter._running

        await adapter.stop()
        assert not adapter._running

    @pytest.mark.asyncio
    async def test_simulate_inbound(self):
        """Test simulating inbound messages."""
        adapter = MockChannelAdapter("test")
        await adapter.start()

        msg = await adapter.simulate_inbound(
            user_id="user1", text="Hello", thread_id="thread1"
        )

        assert msg.channel == "test"
        assert msg.user_id == "user1"
        assert msg.text == "Hello"
        assert msg.thread_id == "thread1"

        assert len(adapter.inbound_messages) == 1
        assert adapter.inbound_messages[0] == msg

    @pytest.mark.asyncio
    async def test_send_outbound(self):
        """Test sending outbound messages."""
        adapter = MockChannelAdapter("test")
        await adapter.start()

        delivery_ctx = {"user_id": "user1", "chat_id": 123}
        msg = OutboundMessage(text="Response")

        await adapter.send(delivery_ctx, msg)

        assert len(adapter.outbound_messages) == 1
        assert adapter.outbound_messages[0].text == "Response"
        assert adapter.delivery_contexts[0] == delivery_ctx

    @pytest.mark.asyncio
    async def test_send_before_start_raises(self):
        """Test that sending before start raises an error."""
        adapter = MockChannelAdapter("test")

        with pytest.raises(RuntimeError, match="not started"):
            await adapter.send({}, OutboundMessage(text="Test"))

    @pytest.mark.asyncio
    async def test_get_last_outbound(self):
        """Test getting the last outbound message."""
        adapter = MockChannelAdapter("test")
        await adapter.start()

        assert adapter.get_last_outbound() is None

        await adapter.send({}, OutboundMessage(text="First"))
        assert adapter.get_last_outbound().text == "First"

        await adapter.send({}, OutboundMessage(text="Second"))
        assert adapter.get_last_outbound().text == "Second"

    @pytest.mark.asyncio
    async def test_get_outbound_to_user(self):
        """Test filtering outbound messages by user."""
        adapter = MockChannelAdapter("test")
        await adapter.start()

        await adapter.send({"user_id": "alice"}, OutboundMessage(text="Hello Alice"))
        await adapter.send({"user_id": "bob"}, OutboundMessage(text="Hello Bob"))
        await adapter.send({"user_id": "alice"}, OutboundMessage(text="Alice again"))

        alice_msgs = adapter.get_outbound_to_user("alice")
        assert len(alice_msgs) == 2
        assert alice_msgs[0].text == "Hello Alice"
        assert alice_msgs[1].text == "Alice again"

        bob_msgs = adapter.get_outbound_to_user("bob")
        assert len(bob_msgs) == 1
        assert bob_msgs[0].text == "Hello Bob"

    @pytest.mark.asyncio
    async def test_clear(self):
        """Test clearing stored messages."""
        adapter = MockChannelAdapter("test")
        await adapter.start()

        await adapter.simulate_inbound("user1", "Test")
        await adapter.send({"user_id": "user1"}, OutboundMessage(text="Response"))

        assert len(adapter.inbound_messages) == 1
        assert len(adapter.outbound_messages) == 1

        adapter.clear()

        assert len(adapter.inbound_messages) == 0
        assert len(adapter.outbound_messages) == 0
        assert len(adapter.delivery_contexts) == 0

    @pytest.mark.asyncio
    async def test_multiple_inbound_messages(self):
        """Test handling multiple inbound messages."""
        adapter = MockChannelAdapter("test")
        await adapter.start()

        for i in range(5):
            await adapter.simulate_inbound(f"user{i}", f"Message {i}")

        assert len(adapter.inbound_messages) == 5
        assert adapter.inbound_messages[2].user_id == "user2"
        assert adapter.inbound_messages[2].text == "Message 2"
