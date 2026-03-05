"""Tests for additional MCPCommandController methods."""
from unittest.mock import Mock, patch, ANY
import pytest

from swecli.ui_textual.controllers.mcp_command_controller import MCPCommandController

@pytest.fixture
def mock_app():
    app = Mock()
    app.push_screen = Mock()
    app._loop = Mock()
    # Ensure app.conversation is mocked as it's used in handle_connect
    app.conversation = Mock()
    return app

@pytest.fixture
def mock_repl():
    repl = Mock()
    repl.mcp_manager = Mock()
    return repl

@pytest.fixture
def controller(mock_app, mock_repl):
    return MCPCommandController(mock_app, mock_repl)

def test_handle_view_opens_modal(controller, mock_app, mock_repl):
    """Test /mcp view opens the modal."""
    # Setup MCP data
    mock_repl.mcp_manager.list_servers.return_value = ["server1"]
    mock_repl.mcp_manager.is_connected.return_value = True
    mock_repl.mcp_manager.get_server_tools.return_value = ["tool1"]

    # Mock MCPViewerModal import within the method
    # Note: We need to patch where it is IMPORTED, which is inside the method.
    # But patching 'swecli.ui_textual.modals.mcp_viewer_modal.MCPViewerModal'
    # works if the module is loaded.

    # Actually, since it's a local import `from ... import ...`, we need to patch
    # the module `swecli.ui_textual.controllers.mcp_command_controller.MCPViewerModal`
    # BUT that name only exists inside the function scope.
    # So we must patch `swecli.ui_textual.modals.mcp_viewer_modal.MCPViewerModal`.

    with patch("swecli.ui_textual.modals.mcp_viewer_modal.MCPViewerModal") as MockModal:
        controller.handle_view("/mcp view")

        # Verify modal instantiated with correct data
        expected_data = [{
            "name": "server1",
            "connected": True,
            "tool_count": 1,
            "tools": ["tool1"]
        }]
        MockModal.assert_called_with(expected_data)

        # Verify modal pushed to screen
        mock_app.push_screen.assert_called_with(MockModal.return_value)

def test_start_auto_connect_thread(controller, mock_repl):
    """Test start_auto_connect_thread logic."""
    # We mock threading.Thread to avoid actually spawning threads
    with patch("threading.Thread") as MockThread:
        controller.start_auto_connect_thread()

        MockThread.assert_called_once()
        args = MockThread.call_args[1]
        assert args["target"] == controller._launch_auto_connect
        assert args["daemon"] is True
        MockThread.return_value.start.assert_called_once()

def test_launch_auto_connect_logic(controller, mock_repl):
    """Test the logic inside _launch_auto_connect."""
    # This method is called by the thread. We test the logic directly.

    # Setup: 2 servers, one connected, one disconnected
    mock_repl.mcp_manager.list_servers.return_value = ["connected_srv", "new_srv"]

    # is_connected side effect
    mock_repl.mcp_manager.is_connected.side_effect = lambda name: name == "connected_srv"

    # connect_sync return value for new_srv
    mock_repl.mcp_manager.connect_sync.return_value = True
    mock_repl.mcp_manager.get_server_tools.return_value = ["tool1"]

    # Callback for console output
    mock_callback = Mock()
    controller._enqueue_console_text_callback = mock_callback

    # Run logic
    controller._launch_auto_connect()

    # Should attempt to connect to 'new_srv' ONLY
    mock_repl.mcp_manager.connect_sync.assert_called_once_with("new_srv")

    # Should log success message
    mock_callback.assert_called_once()
    assert "MCP (new_srv)" in mock_callback.call_args[0][0]

    # Should refresh tooling
    mock_repl._refresh_runtime_tooling.assert_called_once()
