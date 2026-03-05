"""Tests for input provenance tracking."""

from datetime import datetime

import pytest

from swecli.models.message import ChatMessage, InputProvenance, Role


class TestInputProvenance:
    """Test InputProvenance model."""

    def test_creates_provenance_with_kind(self):
        """Test creating provenance with required kind field."""
        provenance = InputProvenance(kind="external_user")
        assert provenance.kind == "external_user"
        assert provenance.source_channel is None
        assert provenance.source_session_id is None

    def test_creates_provenance_with_channel(self):
        """Test creating provenance with source channel."""
        provenance = InputProvenance(
            kind="external_user",
            source_channel="telegram",
        )
        assert provenance.kind == "external_user"
        assert provenance.source_channel == "telegram"

    def test_creates_provenance_with_session_id(self):
        """Test creating provenance with source session ID."""
        provenance = InputProvenance(
            kind="inter_session",
            source_channel="web",
            source_session_id="abc123",
        )
        assert provenance.kind == "inter_session"
        assert provenance.source_channel == "web"
        assert provenance.source_session_id == "abc123"

    def test_timestamp_auto_generated(self):
        """Test that timestamp is automatically generated."""
        before = datetime.now()
        provenance = InputProvenance(kind="external_user")
        after = datetime.now()

        assert before <= provenance.timestamp <= after

    def test_serializes_to_dict(self):
        """Test that provenance serializes correctly."""
        provenance = InputProvenance(
            kind="external_user",
            source_channel="telegram",
            source_session_id=None,
        )
        data = provenance.model_dump()

        assert data["kind"] == "external_user"
        assert data["source_channel"] == "telegram"
        assert data["source_session_id"] is None
        assert "timestamp" in data


class TestChatMessageProvenance:
    """Test ChatMessage with provenance field."""

    def test_creates_message_without_provenance(self):
        """Test creating message without provenance (backward compatible)."""
        message = ChatMessage(
            role=Role.USER,
            content="Hello",
        )
        assert message.provenance is None

    def test_creates_message_with_provenance(self):
        """Test creating message with provenance."""
        provenance = InputProvenance(
            kind="external_user",
            source_channel="telegram",
        )
        message = ChatMessage(
            role=Role.USER,
            content="Hello",
            provenance=provenance,
        )
        assert message.provenance is not None
        assert message.provenance.kind == "external_user"
        assert message.provenance.source_channel == "telegram"

    def test_serializes_message_with_provenance(self):
        """Test that message with provenance serializes correctly."""
        provenance = InputProvenance(
            kind="external_user",
            source_channel="whatsapp",
            source_session_id=None,
        )
        message = ChatMessage(
            role=Role.USER,
            content="Hello from WhatsApp",
            provenance=provenance,
        )
        data = message.model_dump()

        assert "provenance" in data
        assert data["provenance"]["kind"] == "external_user"
        assert data["provenance"]["source_channel"] == "whatsapp"


class TestProvenanceKinds:
    """Test different provenance kinds and their use cases."""

    def test_external_user_provenance(self):
        """Test external_user provenance for real user messages."""
        provenance = InputProvenance(
            kind="external_user",
            source_channel="telegram",
        )
        message = ChatMessage(
            role=Role.USER,
            content="User query via Telegram",
            provenance=provenance,
        )

        assert message.provenance.kind == "external_user"
        assert message.provenance.source_channel == "telegram"
        assert message.provenance.source_session_id is None

    def test_inter_session_provenance(self):
        """Test inter_session provenance for forwarded messages."""
        provenance = InputProvenance(
            kind="inter_session",
            source_channel="web",
            source_session_id="session-abc-123",
        )
        message = ChatMessage(
            role=Role.USER,
            content="Forwarded message from another session",
            provenance=provenance,
        )

        assert message.provenance.kind == "inter_session"
        assert message.provenance.source_channel == "web"
        assert message.provenance.source_session_id == "session-abc-123"

    def test_internal_system_provenance(self):
        """Test internal_system provenance for system-generated messages."""
        provenance = InputProvenance(
            kind="internal_system",
        )
        message = ChatMessage(
            role=Role.SYSTEM,
            content="System-generated reminder",
            provenance=provenance,
        )

        assert message.provenance.kind == "internal_system"
        assert message.provenance.source_channel is None
        assert message.provenance.source_session_id is None


class TestProvenanceFiltering:
    """Test filtering messages by provenance."""

    def test_identifies_external_user_messages(self):
        """Test identifying external user messages."""
        messages = [
            ChatMessage(
                role=Role.USER,
                content="User 1",
                provenance=InputProvenance(kind="external_user", source_channel="telegram"),
            ),
            ChatMessage(
                role=Role.USER,
                content="Forwarded",
                provenance=InputProvenance(kind="inter_session", source_session_id="123"),
            ),
            ChatMessage(
                role=Role.USER,
                content="User 2",
                provenance=InputProvenance(kind="external_user", source_channel="whatsapp"),
            ),
        ]

        external = [m for m in messages if m.provenance and m.provenance.kind == "external_user"]
        assert len(external) == 2
        assert external[0].content == "User 1"
        assert external[1].content == "User 2"

    def test_identifies_inter_session_messages(self):
        """Test identifying inter-session messages."""
        messages = [
            ChatMessage(
                role=Role.USER,
                content="User 1",
                provenance=InputProvenance(kind="external_user", source_channel="telegram"),
            ),
            ChatMessage(
                role=Role.USER,
                content="Forwarded 1",
                provenance=InputProvenance(
                    kind="inter_session", source_session_id="abc", source_channel="web"
                ),
            ),
            ChatMessage(
                role=Role.USER,
                content="Forwarded 2",
                provenance=InputProvenance(
                    kind="inter_session", source_session_id="def", source_channel="cli"
                ),
            ),
        ]

        forwarded = [m for m in messages if m.provenance and m.provenance.kind == "inter_session"]
        assert len(forwarded) == 2
        assert forwarded[0].provenance.source_session_id == "abc"
        assert forwarded[1].provenance.source_session_id == "def"

    def test_filters_messages_by_source_channel(self):
        """Test filtering messages by source channel."""
        messages = [
            ChatMessage(
                role=Role.USER,
                content="Telegram msg",
                provenance=InputProvenance(kind="external_user", source_channel="telegram"),
            ),
            ChatMessage(
                role=Role.USER,
                content="WhatsApp msg",
                provenance=InputProvenance(kind="external_user", source_channel="whatsapp"),
            ),
            ChatMessage(
                role=Role.USER,
                content="Another Telegram msg",
                provenance=InputProvenance(kind="external_user", source_channel="telegram"),
            ),
        ]

        telegram_msgs = [
            m
            for m in messages
            if m.provenance and m.provenance.source_channel == "telegram"
        ]
        assert len(telegram_msgs) == 2
        assert all("Telegram" in m.content for m in telegram_msgs)


class TestProvenanceLoopPrevention:
    """Test provenance use for preventing message loops."""

    def test_prevents_reprocessing_inter_session_messages(self):
        """Test that inter_session messages can be filtered to prevent loops."""

        def should_process_message(message: ChatMessage) -> bool:
            """Check if message should be processed (not already handled)."""
            if not message.provenance:
                return True  # No provenance - process it
            if message.provenance.kind == "inter_session":
                return False  # Already processed in another session
            return True

        # External user message - should process
        user_msg = ChatMessage(
            role=Role.USER,
            content="Direct user input",
            provenance=InputProvenance(kind="external_user", source_channel="telegram"),
        )
        assert should_process_message(user_msg) is True

        # Inter-session message - should NOT process (prevents loop)
        forwarded_msg = ChatMessage(
            role=Role.USER,
            content="Forwarded message",
            provenance=InputProvenance(kind="inter_session", source_session_id="123"),
        )
        assert should_process_message(forwarded_msg) is False

        # System message - should process
        system_msg = ChatMessage(
            role=Role.SYSTEM,
            content="System notification",
            provenance=InputProvenance(kind="internal_system"),
        )
        assert should_process_message(system_msg) is True

    def test_tracks_message_forwarding_chain(self):
        """Test tracking messages as they're forwarded between sessions."""
        # Original message from user
        original = ChatMessage(
            role=Role.USER,
            content="Original message",
            provenance=InputProvenance(
                kind="external_user",
                source_channel="telegram",
            ),
        )

        # Message forwarded to another session
        forwarded = ChatMessage(
            role=Role.USER,
            content=original.content,
            provenance=InputProvenance(
                kind="inter_session",
                source_channel=original.provenance.source_channel,
                source_session_id="session-1",
            ),
        )

        # Verify forwarding chain
        assert original.provenance.kind == "external_user"
        assert forwarded.provenance.kind == "inter_session"
        assert forwarded.provenance.source_channel == original.provenance.source_channel
        assert forwarded.provenance.source_session_id == "session-1"
