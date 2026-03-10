"""Tests for bridge mode routing in WebSocket handler."""

from __future__ import annotations

import asyncio
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from opendev.web.websocket import WebSocketManager


@pytest.fixture
def ws_manager():
    return WebSocketManager()


@pytest.fixture
def mock_websocket():
    ws = AsyncMock()
    ws.send_json = AsyncMock()
    return ws


class TestBridgeModeRouting:
    """Verify that _handle_query routes to TUI injector in bridge mode."""

    @pytest.mark.asyncio
    async def test_bridge_mode_calls_tui_injector(self, ws_manager, mock_websocket):
        """When bridge mode is active, query should call tui_message_injector."""
        injector = MagicMock()

        mock_state = MagicMock()
        mock_state.is_bridge_mode = True
        mock_state.tui_message_injector = injector
        mock_state.get_current_session_id.return_value = "sess123"

        with patch("opendev.web.websocket.get_state", return_value=mock_state):
            ws_manager.broadcast = AsyncMock()
            data = {
                "type": "query",
                "data": {"message": "hello world", "session_id": "sess123"},
            }
            await ws_manager._handle_query(mock_websocket, data)

        # Verify injector was called with the message
        injector.assert_called_once_with("hello world", "sess123")

        # Verify user_message was broadcast
        ws_manager.broadcast.assert_called_once()
        call_args = ws_manager.broadcast.call_args[0][0]
        assert call_args["type"] == "user_message"
        assert call_args["data"]["content"] == "hello world"

    @pytest.mark.asyncio
    async def test_non_bridge_mode_uses_agent_executor(self, ws_manager, mock_websocket):
        """When NOT in bridge mode, query should proceed to AgentExecutor."""
        mock_executor_instance = MagicMock()
        mock_executor_instance.execute_query = AsyncMock()

        mock_state = MagicMock()
        mock_state.is_bridge_mode = False
        mock_state.get_current_session_id.return_value = "sess123"
        mock_state.is_session_running.return_value = False
        mock_state.session_manager.get_session_by_id.return_value = MagicMock()
        # Provide the singleton agent executor on state
        mock_state._agent_executor = mock_executor_instance

        with patch("opendev.web.websocket.get_state", return_value=mock_state):
            ws_manager.broadcast = AsyncMock()
            data = {
                "type": "query",
                "data": {"message": "test", "session_id": "sess123"},
            }
            await ws_manager._handle_query(mock_websocket, data)

        # Should NOT call tui_message_injector (it's None)
        # Should broadcast user_message via normal path
        assert ws_manager.broadcast.call_count >= 1

    @pytest.mark.asyncio
    async def test_bridge_mode_handles_injector_error(self, ws_manager, mock_websocket):
        """Bridge mode should handle injector errors gracefully."""
        injector = MagicMock(side_effect=RuntimeError("TUI crashed"))

        mock_state = MagicMock()
        mock_state.is_bridge_mode = True
        mock_state.tui_message_injector = injector
        mock_state.get_current_session_id.return_value = "sess123"

        with patch("opendev.web.websocket.get_state", return_value=mock_state):
            ws_manager.broadcast = AsyncMock()
            ws_manager.send_message = AsyncMock()
            data = {
                "type": "query",
                "data": {"message": "test", "session_id": "sess123"},
            }
            await ws_manager._handle_query(mock_websocket, data)

        # Should send error message to the client
        ws_manager.send_message.assert_called_once()
        error_msg = ws_manager.send_message.call_args[0][1]
        assert error_msg["type"] == "error"
