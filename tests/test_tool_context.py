import pytest
from unittest.mock import MagicMock
from swecli.core.context_engineering.tools.context import ToolExecutionContext

def test_tool_execution_context_defaults():
    """Verify ToolExecutionContext defaults to None/False."""
    context = ToolExecutionContext()

    assert context.mode_manager is None
    assert context.approval_manager is None
    assert context.undo_manager is None
    assert context.task_monitor is None
    assert context.session_manager is None
    assert context.ui_callback is None
    assert context.is_subagent is False

def test_tool_execution_context_instantiation():
    """Verify ToolExecutionContext stores values correctly."""
    mock_manager = MagicMock()
    mock_callback = MagicMock()

    context = ToolExecutionContext(
        mode_manager=mock_manager,
        ui_callback=mock_callback,
        is_subagent=True
    )

    assert context.mode_manager is mock_manager
    assert context.ui_callback is mock_callback
    assert context.is_subagent is True
    # Others should still be None
    assert context.approval_manager is None
