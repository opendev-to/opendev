"""Tests for message router and workspace selector."""

import tempfile
from pathlib import Path

import pytest

from swecli.core.channels.base import InboundMessage, OutboundMessage
from swecli.core.channels.mock import MockChannelAdapter
from swecli.core.channels.router import MessageRouter
from swecli.core.channels.workspace_selector import WorkspaceSelector
from swecli.core.context_engineering.history.session_manager import SessionManager


@pytest.fixture
def temp_session_dir():
    """Create a temporary session directory."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


@pytest.fixture
def session_manager(temp_session_dir):
    """Create a session manager."""
    return SessionManager(session_dir=temp_session_dir)


@pytest.fixture
def mock_adapter():
    """Create a mock channel adapter."""
    return MockChannelAdapter("test-channel")


class TestWorkspaceSelector:
    """Test WorkspaceSelector functionality."""

    @pytest.mark.asyncio
    async def test_prompt_with_no_workspaces(self, session_manager, mock_adapter):
        """Test prompting when no workspaces exist."""
        selector = WorkspaceSelector(session_manager)
        await mock_adapter.start()

        await selector.prompt_workspace_selection(
            mock_adapter, {"user_id": "user1"}, workspaces=[]
        )

        # Should send prompt asking for path
        assert len(mock_adapter.outbound_messages) == 1
        msg = mock_adapter.outbound_messages[0]
        assert "full path" in msg.text.lower()
        assert "project directory" in msg.text.lower()

    @pytest.mark.asyncio
    async def test_prompt_with_existing_workspaces(self, session_manager, mock_adapter):
        """Test prompting when workspaces exist."""
        selector = WorkspaceSelector(session_manager)
        await mock_adapter.start()

        workspaces = ["/project1", "/project2", "/project3"]
        await selector.prompt_workspace_selection(
            mock_adapter, {"user_id": "user1"}, workspaces=workspaces
        )

        # Should list workspaces with numbers
        msg = mock_adapter.outbound_messages[0]
        assert "1." in msg.text
        assert "2." in msg.text
        assert "3." in msg.text
        assert "/project1" in msg.text

    def test_parse_number_selection(self, session_manager):
        """Test parsing numeric workspace selection."""
        selector = WorkspaceSelector(session_manager)
        workspaces = ["/project1", "/project2", "/project3"]

        result = selector.parse_workspace_choice("2", workspaces)
        assert result == "/project2"

        result = selector.parse_workspace_choice("1", workspaces)
        assert result == "/project1"

    def test_parse_invalid_number(self, session_manager):
        """Test parsing invalid number selection."""
        selector = WorkspaceSelector(session_manager)
        workspaces = ["/project1", "/project2"]

        result = selector.parse_workspace_choice("0", workspaces)
        assert result is None

        result = selector.parse_workspace_choice("5", workspaces)
        assert result is None

    def test_parse_valid_path(self, session_manager, temp_session_dir):
        """Test parsing direct path selection."""
        selector = WorkspaceSelector(session_manager)

        # Use temp_session_dir as a valid path
        result = selector.parse_workspace_choice(str(temp_session_dir), [])
        # Compare resolved paths (macOS symlinks /var -> /private/var)
        assert Path(result).resolve() == temp_session_dir.resolve()

    def test_parse_invalid_path(self, session_manager):
        """Test parsing invalid path selection."""
        selector = WorkspaceSelector(session_manager)

        result = selector.parse_workspace_choice("/nonexistent/path", [])
        assert result is None


class TestMessageRouter:
    """Test MessageRouter functionality."""

    @pytest.mark.asyncio
    async def test_register_adapter(self, session_manager):
        """Test registering channel adapters."""
        router = MessageRouter(session_manager)
        adapter = MockChannelAdapter("telegram")

        router.register_adapter(adapter)

        assert router.get_adapter("telegram") == adapter
        assert router.get_adapter("whatsapp") is None

    @pytest.mark.asyncio
    async def test_create_new_session_for_new_user(self, session_manager, mock_adapter):
        """Test that router creates new session for new users."""
        router = MessageRouter(session_manager)
        router.register_adapter(mock_adapter)
        await mock_adapter.start()

        message = InboundMessage(
            channel="test-channel", user_id="new-user", text="Hello"
        )

        await router.handle_inbound(message)

        # Should create session and prompt for workspace
        assert len(mock_adapter.outbound_messages) == 1
        prompt = mock_adapter.outbound_messages[0]
        assert "workspace" in prompt.text.lower() or "project" in prompt.text.lower()

    @pytest.mark.asyncio
    async def test_workspace_selection_flow(self, session_manager, mock_adapter, temp_session_dir):
        """Test complete workspace selection flow."""
        router = MessageRouter(session_manager)
        router.register_adapter(mock_adapter)
        await mock_adapter.start()

        # First message - should prompt for workspace
        msg1 = InboundMessage(channel="test-channel", user_id="user1", text="Hello")
        await router.handle_inbound(msg1)

        assert len(mock_adapter.outbound_messages) >= 1
        # First message should be workspace prompt
        assert "workspace" in mock_adapter.outbound_messages[0].text.lower() or "project" in mock_adapter.outbound_messages[0].text.lower()

        mock_adapter.clear()

        # User selects workspace - use resolved path to avoid symlink issues
        workspace_path = str(temp_session_dir.resolve())
        msg2 = InboundMessage(
            channel="test-channel", user_id="user1", text=workspace_path
        )
        await router.handle_inbound(msg2)

        # Should confirm workspace selection
        assert len(mock_adapter.outbound_messages) >= 1
        # Check if any message contains confirmation
        texts = [msg.text for msg in mock_adapter.outbound_messages]
        assert any("✅" in text or "workspace set" in text.lower() for text in texts)

    @pytest.mark.asyncio
    async def test_invalid_workspace_selection_reprompts(self, session_manager, mock_adapter):
        """Test that invalid workspace selection reprompts user."""
        router = MessageRouter(session_manager)
        router.register_adapter(mock_adapter)
        await mock_adapter.start()

        # First message
        msg1 = InboundMessage(channel="test-channel", user_id="user1", text="Hello")
        await router.handle_inbound(msg1)
        mock_adapter.clear()

        # Invalid workspace selection
        msg2 = InboundMessage(
            channel="test-channel", user_id="user1", text="/invalid/path"
        )
        await router.handle_inbound(msg2)

        # Should send error and/or reprompt
        assert len(mock_adapter.outbound_messages) >= 1
        texts = " ".join(msg.text.lower() for msg in mock_adapter.outbound_messages)
        assert "❌" in texts or "invalid" in texts or "workspace" in texts

    @pytest.mark.asyncio
    async def test_resolve_existing_session(self, session_manager, mock_adapter, temp_session_dir):
        """Test that router resolves to existing session for known user."""
        router = MessageRouter(session_manager)
        router.register_adapter(mock_adapter)
        await mock_adapter.start()

        # Create existing session
        session = session_manager.create_session(
            working_directory=str(temp_session_dir),
            channel="test-channel",
            channel_user_id="known-user",
            workspace_confirmed=True,
        )
        from swecli.models.message import ChatMessage, Role

        session.add_message(ChatMessage(role=Role.USER, content="Previous"))
        session_manager.save_session(session)

        # Message from known user
        message = InboundMessage(
            channel="test-channel", user_id="known-user", text="New message"
        )

        # Mock agent executor
        async def mock_agent(session, message_text):
            return f"Echo: {message_text}"

        router._agent_executor = mock_agent

        await router.handle_inbound(message)

        # Should resolve to existing session and execute agent
        assert len(mock_adapter.outbound_messages) == 1
        response = mock_adapter.outbound_messages[0]
        assert "Echo: New message" in response.text

    @pytest.mark.asyncio
    async def test_different_users_get_different_sessions(
        self, session_manager, mock_adapter, temp_session_dir
    ):
        """Test that different users get different sessions."""
        router = MessageRouter(session_manager)
        router.register_adapter(mock_adapter)
        await mock_adapter.start()

        # First user
        msg1 = InboundMessage(channel="test-channel", user_id="alice", text="Hello")
        await router.handle_inbound(msg1)

        # Second user
        msg2 = InboundMessage(channel="test-channel", user_id="bob", text="Hi")
        await router.handle_inbound(msg2)

        # Should send workspace prompts (sessions created but not saved until workspace confirmed)
        # Check that different prompts were sent (indicating different sessions)
        assert len(mock_adapter.outbound_messages) >= 2

        # Verify sessions were created (even if not yet saved)
        # Check via router's internal tracking
        assert len(router._pending_workspace_selection) == 2

    @pytest.mark.asyncio
    async def test_thread_separation(self, session_manager, mock_adapter, temp_session_dir):
        """Test that different threads get different sessions."""
        router = MessageRouter(session_manager)
        router.register_adapter(mock_adapter)
        await mock_adapter.start()

        # Same user, different threads
        msg1 = InboundMessage(
            channel="test-channel", user_id="alice", text="Topic 1", thread_id="thread1"
        )
        await router.handle_inbound(msg1)

        msg2 = InboundMessage(
            channel="test-channel", user_id="alice", text="Topic 2", thread_id="thread2"
        )
        await router.handle_inbound(msg2)

        # Should create 2 different sessions (one per thread)
        # Verify via pending workspace selections
        assert len(router._pending_workspace_selection) == 2

        # Verify thread IDs are different
        pending_keys = list(router._pending_workspace_selection.keys())
        assert len(pending_keys) == 2
        threads = {key[2] for key in pending_keys}  # Extract thread_id from (channel, user_id, thread_id)
        assert threads == {"thread1", "thread2"}

    @pytest.mark.asyncio
    async def test_expired_session_triggers_reset(self, session_manager, mock_adapter, temp_session_dir):
        """Test that expired sessions trigger reset per channel policy."""
        from datetime import datetime, timedelta

        # Use telegram adapter to test idle policy
        from swecli.core.channels.mock import MockChannelAdapter

        telegram_adapter = MockChannelAdapter("telegram")
        router = MessageRouter(session_manager)
        router.register_adapter(telegram_adapter)
        await telegram_adapter.start()

        # Create existing session with old activity (telegram has 60 min idle policy)
        old_time = datetime.utcnow() - timedelta(minutes=90)
        session = session_manager.create_session(
            working_directory=str(temp_session_dir),
            channel="telegram",
            channel_user_id="user1",
            workspace_confirmed=True,
        )
        session.last_activity = old_time
        session_manager.save_session(session)

        # Send new message - should trigger reset
        msg = InboundMessage(channel="telegram", user_id="user1", text="Hello again")
        await router.handle_inbound(msg)

        # Should prompt for workspace (new session created)
        assert len(telegram_adapter.outbound_messages) >= 1
        first_msg = telegram_adapter.outbound_messages[0].text.lower()
        assert "workspace" in first_msg or "project" in first_msg

    @pytest.mark.asyncio
    async def test_recent_session_not_reset(self, session_manager, mock_adapter, temp_session_dir):
        """Test that recent sessions are not reset."""
        from datetime import datetime, timedelta
        from swecli.core.channels.mock import MockChannelAdapter

        # Use telegram adapter to test idle policy
        telegram_adapter = MockChannelAdapter("telegram")
        router = MessageRouter(session_manager)
        router.register_adapter(telegram_adapter)
        await telegram_adapter.start()

        # Create existing session with recent activity
        recent_time = datetime.utcnow() - timedelta(minutes=5)
        session = session_manager.create_session(
            working_directory=str(temp_session_dir),
            channel="telegram",
            channel_user_id="user1",
            workspace_confirmed=True,
        )
        session.last_activity = recent_time
        from swecli.models.message import ChatMessage, Role

        session.add_message(ChatMessage(role=Role.USER, content="Previous"))
        session_manager.save_session(session)

        # Mock agent executor
        async def mock_agent(session, message_text):
            return f"Echo: {message_text}"

        router._agent_executor = mock_agent

        # Send new message - should NOT reset
        msg = InboundMessage(channel="telegram", user_id="user1", text="Hello again")
        await router.handle_inbound(msg)

        # Should execute agent (not prompt for workspace)
        assert len(telegram_adapter.outbound_messages) == 1
        response = telegram_adapter.outbound_messages[0]
        assert "Echo: Hello again" in response.text

    @pytest.mark.asyncio
    async def test_router_tags_messages_with_provenance(self, session_manager, temp_session_dir):
        """Test that router tags incoming messages with provenance."""
        from swecli.core.channels.mock import MockChannelAdapter

        telegram_adapter = MockChannelAdapter("telegram")
        router = MessageRouter(session_manager)
        router.register_adapter(telegram_adapter)
        await telegram_adapter.start()

        # Create session with a message
        from swecli.models.message import ChatMessage, Role

        session = session_manager.create_session(
            working_directory=str(temp_session_dir),
            channel="telegram",
            channel_user_id="user1",
            workspace_confirmed=True,
        )
        session.add_message(ChatMessage(role=Role.USER, content="Initial"))
        session_manager.save_session(session)

        # Track the session that the agent sees
        captured_session = None

        # Mock agent executor
        async def mock_agent(session_arg, message_text):
            nonlocal captured_session
            captured_session = session_arg
            return "Response"

        router._agent_executor = mock_agent

        # Send message
        msg = InboundMessage(channel="telegram", user_id="user1", text="Test message")
        await router.handle_inbound(msg)

        # Verify message was added with provenance
        assert captured_session is not None
        assert len(captured_session.messages) >= 1
        user_msg = captured_session.messages[-1]
        assert user_msg.provenance is not None
        assert user_msg.provenance.kind == "external_user"
        assert user_msg.provenance.source_channel == "telegram"
        assert user_msg.provenance.source_session_id is None
