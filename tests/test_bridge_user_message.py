"""Verify _web_broadcast_sync sends user_message when web bridge is active."""

import asyncio
from unittest.mock import MagicMock, AsyncMock, patch

from opendev.ui_textual.runner import TextualRunner


class TestUserMessageBroadcast:
    """Fix 1: TUI user messages are broadcast to web frontend."""

    def _make_runner(self, tmp_path):
        """Create a minimal TextualRunner with mocked dependencies."""
        sm = MagicMock()
        session = MagicMock()
        session.id = "test-sess-123"
        sm.get_current_session.return_value = session

        runner = TextualRunner.__new__(TextualRunner)
        runner.session_manager = sm
        runner.working_dir = str(tmp_path)
        return runner

    def test_web_broadcast_sync_sends_user_message(self, tmp_path):
        """_web_broadcast_sync called with user_message payload."""
        runner = self._make_runner(tmp_path)

        mock_ws = AsyncMock()
        mock_state = MagicMock()
        mock_state.ws_manager = mock_ws
        loop = asyncio.new_event_loop()
        mock_state.get_event_loop.return_value = loop

        with patch("opendev.web.state.get_state", return_value=mock_state):
            runner._web_broadcast_sync({
                "type": "user_message",
                "data": {
                    "role": "user",
                    "content": "hello from TUI",
                    "session_id": "test-sess-123",
                },
            })

        mock_ws.broadcast.assert_called_once()
        payload = mock_ws.broadcast.call_args[0][0]
        assert payload["type"] == "user_message"
        assert payload["data"]["content"] == "hello from TUI"
        assert payload["data"]["session_id"] == "test-sess-123"
        loop.close()

    def test_web_broadcast_sync_handles_missing_ws_manager(self, tmp_path):
        """_web_broadcast_sync silently handles missing ws_manager."""
        runner = self._make_runner(tmp_path)

        mock_state = MagicMock()
        mock_state.ws_manager = None
        mock_state.get_event_loop.return_value = None

        with patch("opendev.web.state.get_state", return_value=mock_state):
            # Should not raise
            runner._web_broadcast_sync({
                "type": "user_message",
                "data": {"content": "test"},
            })
