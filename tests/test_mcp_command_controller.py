"""Tests for MCPCommandController."""
from unittest.mock import Mock, MagicMock
import pytest

from swecli.ui_textual.controllers.mcp_command_controller import MCPCommandController


@pytest.fixture
def mock_app():
    app = Mock()
    app.conversation = Mock()
    # Controller now uses SpinnerService via app.spinner_service
    app.spinner_service = Mock()
    app.spinner_service.start.return_value = "spinner-id"
    return app


@pytest.fixture
def mock_repl():
    repl = Mock()
    repl.mcp_manager = Mock()
    return repl


@pytest.fixture
def controller(mock_app, mock_repl):
    return MCPCommandController(mock_app, mock_repl)


def test_handle_connect_success(controller, mock_app, mock_repl):
    """Test successful MCP connection."""
    mock_repl.mcp_manager.is_connected.return_value = False
    mock_repl.mcp_manager.connect_sync.return_value = True
    mock_repl.mcp_manager.get_server_tools.return_value = ["tool1", "tool2"]

    controller.handle_connect("/mcp connect github")

    # Verify spinner started
    mock_app.spinner_service.start.assert_called_with("MCP (github)")


def test_handle_connect_failure(controller, mock_app, mock_repl):
    """Test failed MCP connection - spinner service handles the error display."""
    mock_repl.mcp_manager.is_connected.return_value = False
    mock_repl.mcp_manager.connect_sync.return_value = False

    controller.handle_connect("/mcp connect github")

    # Verify spinner started
    mock_app.spinner_service.start.assert_called_with("MCP (github)")


def test_handle_connect_already_connected(controller, mock_app, mock_repl):
    """Test connecting to already connected server."""
    mock_repl.mcp_manager.is_connected.return_value = True
    mock_repl.mcp_manager.get_server_tools.return_value = ["tool1"]

    controller.handle_connect("/mcp connect github")

    # Should not attempt connection
    mock_repl.mcp_manager.connect_sync.assert_not_called()

    # Should use spinner service for already-connected message
    mock_app.spinner_service.start.assert_called_once()
    mock_app.spinner_service.stop.assert_called_once_with(
        "spinner-id", success=True, result_message="Already connected (1 tools)"
    )


def test_handle_connect_invalid_command(controller, mock_app):
    """Test invalid command format."""
    controller.handle_connect("/mcp connect")  # Missing server name

    mock_app.conversation.add_error.assert_called_with("Usage: /mcp connect <server_name>")


def test_handle_connect_no_manager(controller, mock_app, mock_repl):
    """Test when MCP manager is missing."""
    mock_repl.mcp_manager = None

    controller.handle_connect("/mcp connect github")

    mock_app.conversation.add_error.assert_called_with("MCP manager not available")
