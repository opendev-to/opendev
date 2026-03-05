"""Integration tests for todo UI callback flow."""

import pytest
from unittest.mock import MagicMock, patch, PropertyMock

from swecli.core.context_engineering.tools.handlers.todo_handler import TodoHandler


class TestTodoCallbackRefresh:
    """Test that UI callback triggers panel refresh for todo tools."""

    def test_refresh_called_after_write_todos(self):
        """Panel refresh called after write_todos completes."""
        handler = TodoHandler()

        # Simulate write_todos call
        result = handler.write_todos(["Task 1", "Task 2"])

        assert result["success"]
        assert result["created_count"] == 2

        # Verify todos were created
        assert len(handler._todos) == 2

    def test_refresh_called_after_update_todo(self):
        """Panel refresh called after update_todo completes."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])

        # Simulate update_todo call
        result = handler.update_todo("todo-1", status="doing")

        assert result["success"]
        assert handler._todos["todo-1"].status == "doing"

    def test_refresh_called_after_complete_todo(self):
        """Panel refresh called after complete_todo completes."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])

        # Simulate complete_todo call
        result = handler.complete_todo("todo-1")

        assert result["success"]
        assert handler._todos["todo-1"].status == "done"


class TestTodoIDMatching:
    """Test ID matching reliability for todo operations."""

    def test_exact_id_works(self):
        """Exact 'todo-N' ID should work."""
        handler = TodoHandler()
        handler.write_todos(["Task one", "Task two"])

        # Use exact ID
        result = handler.update_todo("todo-1", status="doing")
        assert result["success"]
        assert handler._todos["todo-1"].status == "doing"

    def test_underscore_format_works(self):
        """'todo_N' format (Deep Agent) should be converted."""
        handler = TodoHandler()
        handler.write_todos(["Task one", "Task two"])

        # Use underscore format
        result = handler.update_todo("todo_1", status="doing")
        assert result["success"]
        assert handler._todos["todo-1"].status == "doing"

    def test_numeric_zero_based_works(self):
        """0-based numeric index should work."""
        handler = TodoHandler()
        handler.write_todos(["Task one", "Task two", "Task three"])

        # Use 0-based index (Deep Agent format)
        result = handler.update_todo("0", status="doing")
        assert result["success"]
        assert handler._todos["todo-1"].status == "doing"

        result = handler.update_todo("2", status="doing")
        assert result["success"]
        assert handler._todos["todo-3"].status == "doing"

    def test_partial_match_works(self):
        """Partial string match should work."""
        handler = TodoHandler()
        handler.write_todos(["Implement feature X", "Test feature X"])

        # Use partial match
        result = handler.update_todo("Implement", status="doing")
        assert result["success"]
        assert handler._todos["todo-1"].status == "doing"

    def test_kebab_case_slug_works(self):
        """Kebab-case slug should work."""
        handler = TodoHandler()
        handler.write_todos(["Implement basic level design"])

        # Use kebab-case
        result = handler.update_todo("implement-basic-level", status="doing")
        assert result["success"]
        assert handler._todos["todo-1"].status == "doing"


class TestTodoStateTransitions:
    """Test state transitions for todo lifecycle."""

    def test_transition_pending_to_doing(self):
        """Test transition from pending to doing."""
        handler = TodoHandler()
        handler.write_todos(["My task"])

        # Initial state
        assert handler._todos["todo-1"].status == "todo"

        # Start working
        handler.update_todo("todo-1", status="in_progress")
        assert handler._todos["todo-1"].status == "doing"

    def test_transition_doing_to_done(self):
        """Test transition from doing to done."""
        handler = TodoHandler()
        handler.write_todos(["My task"])
        handler.update_todo("todo-1", status="in_progress")

        # Complete
        handler.complete_todo("todo-1")
        assert handler._todos["todo-1"].status == "done"

    def test_full_lifecycle_pending_to_done(self):
        """Test todo through full lifecycle: pending -> doing -> done."""
        handler = TodoHandler()
        handler.write_todos(["My task"])

        # Initial state
        todo = handler._todos["todo-1"]
        assert todo.status == "todo"

        # Start working
        handler.update_todo("todo-1", status="in_progress")
        assert todo.status == "doing"

        # Complete
        handler.complete_todo("todo-1")
        assert todo.status == "done"

    def test_single_doing_enforcement(self):
        """Only one todo can be 'doing' at a time."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3"])

        handler.update_todo("todo-1", status="doing")
        assert handler._todos["todo-1"].status == "doing"

        # Set another to doing - first should revert
        handler.update_todo("todo-2", status="doing")
        assert handler._todos["todo-1"].status == "todo"  # Reverted
        assert handler._todos["todo-2"].status == "doing"

    def test_complete_and_activate_next(self):
        """Test atomic complete + activate next flow."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3"])
        handler.update_todo("todo-1", status="doing")

        result = handler.complete_and_activate_next("todo-1")

        assert result["success"]
        assert handler._todos["todo-1"].status == "done"
        assert handler._todos["todo-2"].status == "doing"


class TestTodoErrorHandling:
    """Test error handling for invalid operations."""

    def test_update_nonexistent_returns_error(self):
        """Updating non-existent todo returns helpful error."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])

        result = handler.update_todo("todo-999", status="doing")

        assert not result["success"]
        assert "not found" in result["error"]
        assert "Valid IDs" in result["error"]

    def test_complete_nonexistent_returns_error(self):
        """Completing non-existent todo returns helpful error."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])

        result = handler.complete_todo("nonexistent")

        assert not result["success"]
        assert "not found" in result["error"]

    def test_empty_write_todos_returns_error(self):
        """write_todos with empty list returns error."""
        handler = TodoHandler()

        result = handler.write_todos([])

        assert not result["success"]
        assert "No todos provided" in result["error"]

    def test_invalid_status_returns_error(self):
        """Invalid status returns error."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])

        result = handler.update_todo("todo-1", status="invalid_status")

        assert not result["success"]
        assert "Invalid status" in result["error"]
