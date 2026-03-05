"""Tests for ToolCommands handler."""

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from rich.console import Console

from swecli.repl.commands.tool_commands import ToolCommands


@pytest.fixture
def mock_repl():
    """Create mock REPL for ToolCommands."""
    mock = MagicMock()
    mock.config = MagicMock()
    mock.mode_manager = MagicMock()
    mock.approval_manager = MagicMock()
    mock.undo_manager = MagicMock()
    mock.session_manager = MagicMock()
    mock.agent = MagicMock()
    mock.agent.run_sync.return_value = {"success": True}
    return mock


@pytest.fixture
def tool_commands(mock_repl):
    """Create ToolCommands instance."""
    return ToolCommands(console=MagicMock(spec=Console), repl=mock_repl)


def test_init_command(tool_commands, mock_repl, tmp_path):
    """Test /init command runs main agent with init prompt."""
    mock_repl.agent.run_sync.return_value = {"success": True}

    # Create a fake OPENDEV.md to simulate success
    opendev_path = tmp_path / "OPENDEV.md"
    opendev_path.write_text("# Test")

    with patch("swecli.repl.commands.tool_commands.load_prompt") as mock_load:
        mock_load.return_value = "Test prompt for {path}"
        tool_commands.init_codebase(f"/init {tmp_path}")

    # Verify agent.run_sync was called
    mock_repl.agent.run_sync.assert_called_once()
    call_kwargs = mock_repl.agent.run_sync.call_args[1]
    assert "message" in call_kwargs
    assert str(tmp_path) in call_kwargs["message"]


def test_init_command_invalid_path(tool_commands):
    """Test /init command with invalid path."""
    tool_commands.init_codebase("/init /nonexistent/path")


def test_init_command_shows_summary_in_tui(mock_repl, tmp_path):
    """Test /init command shows LLM-generated summary in TUI mode."""
    # Track run_sync calls
    call_count = [0]

    def mock_run_sync(**kwargs):
        call_count[0] += 1
        if call_count[0] == 1:
            # First call - return empty content (simulating write completion)
            return {"success": True, "content": "", "messages": []}
        else:
            # Second call - return summary
            return {"success": True, "content": "Created OPENDEV.md with overview."}

    mock_repl.agent.run_sync = mock_run_sync

    # Create mock ui_callback
    mock_ui_callback = MagicMock()

    tool_commands = ToolCommands(console=MagicMock(spec=Console), repl=mock_repl)
    tool_commands.ui_callback = mock_ui_callback

    # Create OPENDEV.md to simulate success
    opendev_path = tmp_path / "OPENDEV.md"
    opendev_path.write_text("# Test")

    with patch("swecli.repl.commands.tool_commands.load_prompt") as mock_load:
        mock_load.return_value = "Test prompt for {path}"
        tool_commands.init_codebase(f"/init {tmp_path}")

    # Verify summary LLM call was made and displayed
    assert call_count[0] == 2, "Should make 2 LLM calls (main + summary)"
    mock_ui_callback.on_assistant_message.assert_called_once_with(
        "Created OPENDEV.md with overview."
    )
