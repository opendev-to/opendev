"""Unit tests for MainAgent todo completion validation."""

import pytest
from unittest.mock import MagicMock

from swecli.core.context_engineering.tools.handlers.todo_handler import TodoHandler


class MockAgent:
    """Minimal mock that has just the _check_todo_completion method."""

    def __init__(self, tool_registry=None):
        self.tool_registry = tool_registry

    def _check_todo_completion(self) -> tuple[bool, str]:
        """Check if completion is allowed given todo state.

        This is copied from MainAgent to test in isolation.
        """
        if not hasattr(self, "tool_registry") or not self.tool_registry:
            return True, ""

        todo_handler = getattr(self.tool_registry, "todo_handler", None)
        if not todo_handler:
            return True, ""

        if not todo_handler.has_todos():
            return True, ""  # No todos created - OK to complete

        incomplete = todo_handler.get_incomplete_todos()
        if not incomplete:
            return True, ""  # All todos done - OK to complete

        # Build nudge message with incomplete todo titles
        titles = [t.title for t in incomplete[:3]]
        msg = (
            f"⚠️ You have {len(incomplete)} incomplete todo(s):\n"
            + "\n".join(f"  • {title}" for title in titles)
            + ("\n  ..." if len(incomplete) > 3 else "")
            + "\n\nPlease complete these tasks or mark them done before finishing."
        )
        return False, msg


class TestCheckTodoCompletion:
    """Test suite for _check_todo_completion() method logic."""

    def test_no_tool_registry(self):
        """Returns (True, '') when no tool_registry exists."""
        agent = MockAgent(tool_registry=None)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is True
        assert msg == ""

    def test_no_todo_handler(self):
        """Returns (True, '') when todo_handler is None."""
        tool_registry = MagicMock()
        tool_registry.todo_handler = None
        agent = MockAgent(tool_registry=tool_registry)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is True
        assert msg == ""

    def test_no_todos_created(self):
        """Returns (True, '') when no todos have been created."""
        tool_registry = MagicMock()
        tool_registry.todo_handler = TodoHandler()
        agent = MockAgent(tool_registry=tool_registry)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is True
        assert msg == ""

    def test_all_todos_done(self):
        """Returns (True, '') when all todos are completed."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3"])
        handler.complete_todo("todo-1")
        handler.complete_todo("todo-2")
        handler.complete_todo("todo-3")

        tool_registry = MagicMock()
        tool_registry.todo_handler = handler
        agent = MockAgent(tool_registry=tool_registry)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is True
        assert msg == ""

    def test_incomplete_todos_returns_false(self):
        """Returns (False, message) when incomplete todos exist."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])
        handler.complete_todo("todo-1")
        # todo-2 remains pending

        tool_registry = MagicMock()
        tool_registry.todo_handler = handler
        agent = MockAgent(tool_registry=tool_registry)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is False
        assert "1 incomplete todo(s)" in msg
        assert "Task 2" in msg

    def test_nudge_message_format(self):
        """Nudge message includes incomplete todo titles."""
        handler = TodoHandler()
        handler.write_todos(["First task", "Second task", "Third task"])
        # Leave all pending

        tool_registry = MagicMock()
        tool_registry.todo_handler = handler
        agent = MockAgent(tool_registry=tool_registry)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is False
        assert "3 incomplete todo(s)" in msg
        assert "First task" in msg
        assert "Second task" in msg
        assert "Third task" in msg
        assert "Please complete these tasks" in msg

    def test_nudge_message_truncates_at_three(self):
        """Nudge message shows at most 3 todo titles with ellipsis."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3", "Task 4", "Task 5"])
        # Leave all pending

        tool_registry = MagicMock()
        tool_registry.todo_handler = handler
        agent = MockAgent(tool_registry=tool_registry)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is False
        assert "5 incomplete todo(s)" in msg
        assert "Task 1" in msg
        assert "Task 2" in msg
        assert "Task 3" in msg
        assert "..." in msg  # Indicates more items
        # Task 4 and Task 5 should NOT appear (truncated)
        assert "Task 4" not in msg
        assert "Task 5" not in msg

    def test_in_progress_todo_is_incomplete(self):
        """Todos with status='doing' are considered incomplete."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])
        handler.update_todo("todo-1", status="doing")

        tool_registry = MagicMock()
        tool_registry.todo_handler = handler
        agent = MockAgent(tool_registry=tool_registry)

        can_complete, msg = agent._check_todo_completion()
        assert can_complete is False
        assert "1 incomplete todo(s)" in msg
