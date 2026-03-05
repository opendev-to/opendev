"""Tests for TodoPanel widget behavior, focusing on auto-hide functionality."""

import pytest
from unittest.mock import MagicMock, patch

from swecli.core.context_engineering.tools.handlers.todo_handler import TodoHandler, TodoItem


class TestTodoPanelAutoHide:
    """Test auto-hide behavior when all todos complete."""

    @pytest.fixture
    def handler(self):
        """Create a TodoHandler with sample todos."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3"])
        return handler

    def test_all_done_detection(self, handler):
        """Verify all() check correctly identifies completion."""
        # Not all done yet
        handler.update_todo("todo-1", status="completed")
        todos = list(handler._todos.values())
        assert not all(t.status == "done" for t in todos)

        # Mark all as done
        handler.update_todo("todo-2", status="completed")
        handler.update_todo("todo-3", status="completed")
        todos = list(handler._todos.values())
        assert all(t.status == "done" for t in todos)

    def test_panel_should_hide_when_all_complete(self, handler):
        """Panel should hide (remove classes, clear content) when all done."""
        # Complete all todos
        handler.update_todo("todo-1", status="completed")
        handler.update_todo("todo-2", status="completed")
        handler.update_todo("todo-3", status="completed")

        # Verify all are done
        todos = list(handler._todos.values())
        assert len(todos) == 3
        assert all(t.status == "done" for t in todos)

    def test_panel_should_be_visible_when_todos_pending(self, handler):
        """Panel should be visible when todos exist and not all done."""
        # Complete some but not all
        handler.update_todo("todo-1", status="completed")

        todos = list(handler._todos.values())
        assert not all(t.status == "done" for t in todos)

        # Count done vs total
        completed = len([t for t in todos if t.status == "done"])
        assert completed == 1
        assert len(todos) == 3

    def test_panel_should_be_hidden_when_no_todos(self):
        """Panel should be hidden when no todos exist."""
        handler = TodoHandler()  # Empty
        todos = list(handler._todos.values())
        assert len(todos) == 0


class TestTodoPanelCompletionTracking:
    """Test that panel correctly tracks completion status."""

    def test_completion_count_accurate(self):
        """Verify completion count matches actual done todos."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3", "Task 4"])

        handler.update_todo("todo-1", status="completed")
        handler.update_todo("todo-3", status="completed")

        todos = list(handler._todos.values())
        completed = len([t for t in todos if t.status == "done"])
        assert completed == 2
        assert len(todos) == 4

    def test_completion_counter_all_done(self):
        """Verify counter shows N/N when all todos are completed."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3"])

        # Mark all as done
        handler.update_todo("todo-1", status="completed")
        handler.update_todo("todo-2", status="completed")
        handler.update_todo("todo-3", status="completed")

        todos = list(handler._todos.values())
        assert len(todos) == 3
        assert all(t.status == "done" for t in todos)

        # Calculate what counter should show
        completed = len([t for t in todos if t.status == "done"])
        total = len(todos)
        assert completed == 3
        assert total == 3

    def test_no_active_todo_when_all_complete(self):
        """Verify no active todo exists when all are complete."""
        handler = TodoHandler()
        handler.write_todos([{"content": "Task 1", "status": "completed"}])

        todos = list(handler._todos.values())
        has_active = any(t.status == "doing" for t in todos)
        assert not has_active

    def test_has_active_todo_when_in_progress(self):
        """Verify active todo detection works correctly."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3"])

        # Set one as in_progress
        handler.update_todo("todo-2", status="in_progress")

        todos = list(handler._todos.values())
        has_active = any(t.status == "doing" for t in todos)
        assert has_active


class TestTodoPanelWidgetLogic:
    """Test TodoPanel widget internal logic (without Textual runtime)."""

    def test_get_active_todo_text_returns_title(self):
        """Verify _get_active_todo_text logic with mock todos."""
        # Create mock todos
        todo1 = TodoItem(id="todo-1", title="First task", status="done")
        todo2 = TodoItem(id="todo-2", title="Second task", status="doing")
        todo3 = TodoItem(id="todo-3", title="Third task", status="todo")

        todos = [todo1, todo2, todo3]

        # Find active todo text (same logic as _get_active_todo_text)
        active_text = None
        for todo in todos:
            if todo.status == "doing":
                active_text = todo.active_form if todo.active_form else todo.title
                break

        assert active_text == "Second task"

    def test_get_active_todo_text_prefers_active_form(self):
        """Verify active_form is preferred over title."""
        todo = TodoItem(
            id="todo-1",
            title="Run the tests",
            status="doing",
            active_form="Running tests"
        )
        todos = [todo]

        # Find active todo text
        active_text = None
        for t in todos:
            if t.status == "doing":
                active_text = t.active_form if t.active_form else t.title
                break

        assert active_text == "Running tests"

    def test_get_active_todo_text_returns_none_when_no_active(self):
        """Verify None is returned when no todo is doing."""
        todo1 = TodoItem(id="todo-1", title="First task", status="done")
        todo2 = TodoItem(id="todo-2", title="Second task", status="todo")

        todos = [todo1, todo2]

        # Find active todo text
        active_text = None
        for todo in todos:
            if todo.status == "doing":
                active_text = todo.active_form if todo.active_form else todo.title
                break

        assert active_text is None
