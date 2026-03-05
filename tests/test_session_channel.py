"""Tests for multi-channel session functionality."""

import tempfile
from pathlib import Path

import pytest

from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.models.session import Session, SessionMetadata
from swecli.models.message import ChatMessage, Role


@pytest.fixture
def temp_session_dir():
    """Create a temporary session directory."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


def test_create_session_with_channel_fields(temp_session_dir):
    """Test creating a session with channel-specific fields."""
    manager = SessionManager(session_dir=temp_session_dir)

    session = manager.create_session(
        working_directory="/test/project",
        channel="telegram",
        channel_user_id="@testuser",
        chat_type="direct",
        thread_id=None,
        delivery_context={"chat_id": 123456, "platform": "telegram"},
        workspace_confirmed=False,
    )

    assert session.channel == "telegram"
    assert session.channel_user_id == "@testuser"
    assert session.chat_type == "direct"
    assert session.thread_id is None
    assert session.delivery_context == {"chat_id": 123456, "platform": "telegram"}
    assert session.workspace_confirmed is False
    assert session.last_activity is not None


def test_session_metadata_includes_channel_fields(temp_session_dir):
    """Test that SessionMetadata includes channel fields."""
    manager = SessionManager(session_dir=temp_session_dir)

    session = manager.create_session(
        channel="whatsapp",
        channel_user_id="+1234567890",
        thread_id="topic_123",
    )

    metadata = session.get_metadata()

    assert isinstance(metadata, SessionMetadata)
    assert metadata.channel == "whatsapp"
    assert metadata.channel_user_id == "+1234567890"
    assert metadata.thread_id == "topic_123"


def test_find_session_by_channel_user(temp_session_dir):
    """Test finding sessions by channel and user ID."""
    manager = SessionManager(session_dir=temp_session_dir)

    # Create sessions for different channels
    session1 = manager.create_session(
        channel="telegram",
        channel_user_id="@alice",
    )
    session1.add_message(ChatMessage(role=Role.USER, content="Hello"))
    manager.save_session(session1)

    session2 = manager.create_session(
        channel="whatsapp",
        channel_user_id="+9876543210",
    )
    session2.add_message(ChatMessage(role=Role.USER, content="Hi"))
    manager.save_session(session2)

    session3 = manager.create_session(
        channel="telegram",
        channel_user_id="@bob",
    )
    session3.add_message(ChatMessage(role=Role.USER, content="Hey"))
    manager.save_session(session3)

    # Find by channel and user
    found = manager.find_session_by_channel_user("telegram", "@alice")
    assert found is not None
    assert found.id == session1.id
    assert found.channel == "telegram"
    assert found.channel_user_id == "@alice"

    # Find different user on same channel
    found = manager.find_session_by_channel_user("telegram", "@bob")
    assert found is not None
    assert found.id == session3.id

    # Find on different channel
    found = manager.find_session_by_channel_user("whatsapp", "+9876543210")
    assert found is not None
    assert found.id == session2.id

    # Not found
    found = manager.find_session_by_channel_user("telegram", "@charlie")
    assert found is None


def test_find_session_by_channel_user_with_thread(temp_session_dir):
    """Test finding sessions with thread ID."""
    manager = SessionManager(session_dir=temp_session_dir)

    # Create sessions in different threads
    session1 = manager.create_session(
        channel="telegram",
        channel_user_id="@alice",
        thread_id="topic_1",
    )
    session1.add_message(ChatMessage(role=Role.USER, content="Hello"))
    manager.save_session(session1)

    session2 = manager.create_session(
        channel="telegram",
        channel_user_id="@alice",
        thread_id="topic_2",
    )
    session2.add_message(ChatMessage(role=Role.USER, content="Hi"))
    manager.save_session(session2)

    # Find specific thread
    found = manager.find_session_by_channel_user("telegram", "@alice", thread_id="topic_1")
    assert found is not None
    assert found.id == session1.id
    assert found.thread_id == "topic_1"

    # Find different thread
    found = manager.find_session_by_channel_user("telegram", "@alice", thread_id="topic_2")
    assert found is not None
    assert found.id == session2.id
    assert found.thread_id == "topic_2"

    # Find without thread_id (returns any matching session)
    found = manager.find_session_by_channel_user("telegram", "@alice")
    assert found is not None
    assert found.channel_user_id == "@alice"


def test_index_persists_channel_fields(temp_session_dir):
    """Test that channel fields are persisted in the session index."""
    manager = SessionManager(session_dir=temp_session_dir)

    # Create and save session with channel fields
    session = manager.create_session(
        channel="telegram",
        channel_user_id="@testuser",
        thread_id="thread_123",
    )
    session.add_message(ChatMessage(role=Role.USER, content="Test"))
    manager.save_session(session)

    # Create new manager to force index reload
    manager2 = SessionManager(session_dir=temp_session_dir)
    sessions = manager2.list_sessions()

    assert len(sessions) == 1
    metadata = sessions[0]
    assert metadata.channel == "telegram"
    assert metadata.channel_user_id == "@testuser"
    assert metadata.thread_id == "thread_123"


def test_default_channel_values(temp_session_dir):
    """Test that sessions default to CLI channel when not specified."""
    manager = SessionManager(session_dir=temp_session_dir)

    session = manager.create_session(working_directory="/test")

    assert session.channel == "cli"
    assert session.channel_user_id == ""
    assert session.chat_type == "direct"
    assert session.thread_id is None
    assert session.delivery_context == {}
    assert session.workspace_confirmed is True  # CLI defaults to confirmed


def test_list_user_workspaces(temp_session_dir):
    """Test listing available workspaces."""
    # This test requires the full paths setup which is complex
    # For now, test that the method exists and returns a list
    manager = SessionManager(session_dir=temp_session_dir)

    workspaces = manager.list_user_workspaces()
    assert isinstance(workspaces, list)
    # Should return workspace directory names (may include existing projects)
    # Just verify it's a list of strings
    for workspace in workspaces:
        assert isinstance(workspace, str)
