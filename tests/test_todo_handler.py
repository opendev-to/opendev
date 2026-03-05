"""Unit tests for TodoHandler, focusing on _find_todo() lookup logic."""

import pytest
from swecli.core.context_engineering.tools.handlers.todo_handler import TodoHandler, TodoItem


class TestTodoHandlerFindTodo:
    """Test suite for TodoHandler._find_todo() method."""

    @pytest.fixture
    def handler(self):
        """Create a TodoHandler with sample todos."""
        handler = TodoHandler()
        # Create some test todos
        handler.create_todo(title="Set up game development environment and project structure")
        handler.create_todo(title="Design core game mechanics (jumping, running, collision detection)")
        handler.create_todo(title="Implement basic level design with platforms and ground")
        handler.create_todo(title="Add enemy characters (Goombas, Koopas) with basic AI")
        return handler

    def test_find_by_exact_todo_id(self, handler):
        """Test finding todo by exact 'todo-X' format ID."""
        actual_id, todo = handler._find_todo("todo-1")
        assert actual_id == "todo-1"
        assert todo is not None
        assert todo.title == "Set up game development environment and project structure"

    def test_find_by_todo_underscore_format(self, handler):
        """Test finding todo by 'todo_X' format (Deep Agent uses this)."""
        # Deep Agent uses underscore instead of dash
        actual_id, todo = handler._find_todo("todo_1")
        assert actual_id == "todo-1"
        assert todo is not None
        assert todo.title == "Set up game development environment and project structure"

        actual_id, todo = handler._find_todo("todo_3")
        assert actual_id == "todo-3"
        assert todo.title == "Implement basic level design with platforms and ground"

    def test_find_by_numeric_zero_based_index(self, handler):
        """Test finding todo by 0-based numeric index (Deep Agent format)."""
        # Deep Agent uses 0-based indexing, should map to todo-1
        actual_id, todo = handler._find_todo("0")
        assert actual_id == "todo-1"
        assert todo is not None
        assert todo.title == "Set up game development environment and project structure"

        # Index 2 should map to todo-3
        actual_id, todo = handler._find_todo("2")
        assert actual_id == "todo-3"
        assert todo.title == "Implement basic level design with platforms and ground"

    def test_find_by_exact_title_case_sensitive(self, handler):
        """Test finding todo by exact title match (case-sensitive)."""
        exact_title = "Design core game mechanics (jumping, running, collision detection)"
        actual_id, todo = handler._find_todo(exact_title)
        assert actual_id == "todo-2"
        assert todo is not None
        assert todo.title == exact_title

    def test_find_by_exact_title_case_insensitive(self, handler):
        """Test finding todo by exact title match (case-insensitive)."""
        # Try with different casing
        actual_id, todo = handler._find_todo("DESIGN CORE GAME MECHANICS (JUMPING, RUNNING, COLLISION DETECTION)")
        assert actual_id == "todo-2"
        assert todo is not None

        # Try with mixed casing
        actual_id, todo = handler._find_todo("design Core GAME mechanics (jumping, running, collision detection)")
        assert actual_id == "todo-2"
        assert todo is not None

    def test_find_by_kebab_case_slug_exact_prefix(self, handler):
        """Test finding todo by kebab-case slug that matches prefix."""
        # "implement-basic-level" should match "Implement basic level design with platforms and ground"
        actual_id, todo = handler._find_todo("implement-basic-level")
        assert actual_id == "todo-3"
        assert todo is not None
        assert todo.title == "Implement basic level design with platforms and ground"

    def test_find_by_kebab_case_slug_partial_words(self, handler):
        """Test finding todo by kebab-case slug with partial words."""
        # "set-up-game" should match "Set up game development environment and project structure"
        actual_id, todo = handler._find_todo("set-up-game")
        assert actual_id == "todo-1"
        assert todo is not None

        # "enemy-characters" should match "Add enemy characters (Goombas, Koopas) with basic AI"
        actual_id, todo = handler._find_todo("enemy-characters")
        assert actual_id == "todo-4"
        assert todo is not None

    def test_find_by_kebab_case_slug_words_in_order(self, handler):
        """Test finding todo by kebab-case slug where words appear in order."""
        # "core-mechanics-jumping" - words appear in order in the title
        actual_id, todo = handler._find_todo("core-mechanics-jumping")
        assert actual_id == "todo-2"
        assert todo is not None

    def test_find_by_partial_string_match(self, handler):
        """Test finding todo by partial string contained in title."""
        # "level design" is contained in "Implement basic level design with platforms and ground"
        actual_id, todo = handler._find_todo("level design")
        assert actual_id == "todo-3"
        assert todo is not None

        # "goombas" is contained in "Add enemy characters (Goombas, Koopas) with basic AI"
        actual_id, todo = handler._find_todo("goombas")
        assert actual_id == "todo-4"
        assert todo is not None

    def test_find_by_partial_case_insensitive(self, handler):
        """Test partial matching is case-insensitive."""
        actual_id, todo = handler._find_todo("LEVEL DESIGN")
        assert actual_id == "todo-3"
        assert todo is not None

        actual_id, todo = handler._find_todo("GoOmBaS")
        assert actual_id == "todo-4"
        assert todo is not None

    def test_find_not_found_returns_none(self, handler):
        """Test that non-existent IDs return None."""
        actual_id, todo = handler._find_todo("nonexistent-todo")
        assert actual_id is None
        assert todo is None

        actual_id, todo = handler._find_todo("todo-999")
        assert actual_id is None
        assert todo is None

        actual_id, todo = handler._find_todo("99")
        assert actual_id is None
        assert todo is None

    def test_find_empty_string_returns_none(self, handler):
        """Test that empty string returns None."""
        actual_id, todo = handler._find_todo("")
        assert actual_id is None
        assert todo is None

    def test_find_with_special_characters(self, handler):
        """Test finding todos with special characters in title."""
        # The title has parentheses
        actual_id, todo = handler._find_todo("Goombas, Koopas")
        assert actual_id == "todo-4"
        assert todo is not None

    def test_find_kebab_case_vs_exact_match_priority(self, handler):
        """Test that exact matches take priority over fuzzy matches."""
        # Create a todo with kebab-case in the actual title
        handler.create_todo(title="test-kebab-case")

        # Should find exact match first
        actual_id, todo = handler._find_todo("test-kebab-case")
        assert todo.title == "test-kebab-case"


class TestTodoHandlerColors:
    """Test suite for todo color markup."""

    @pytest.fixture
    def handler(self):
        """Create a TodoHandler with todos in different states."""
        handler = TodoHandler()
        handler.create_todo(title="Pending task", status="todo")
        handler.create_todo(title="In progress task", status="doing")
        handler.create_todo(title="Completed task", status="done")
        return handler

    def test_format_todo_list_has_colors(self, handler):
        """Test that formatted todo list includes color markup."""
        formatted = handler._format_todo_list_simple()

        # Should have 3 todos
        assert len(formatted) == 3

        # Check that colors are present
        # In progress (doing) should be yellow
        assert any("[yellow]" in line for line in formatted)

        # Pending (todo) should be cyan
        assert any("[cyan]" in line for line in formatted)

        # Completed (done) should be green
        assert any("[green]" in line for line in formatted)

    def test_pending_todo_is_cyan(self, handler):
        """Test that pending todos use cyan color."""
        formatted = handler._format_todo_list_simple()
        pending_line = [line for line in formatted if "Pending task" in line][0]
        assert "[cyan]" in pending_line
        assert "○" in pending_line
        assert "[/cyan]" in pending_line

    def test_in_progress_todo_is_yellow(self, handler):
        """Test that in-progress todos use yellow color."""
        formatted = handler._format_todo_list_simple()
        in_progress_line = [line for line in formatted if "In progress task" in line][0]
        assert "[yellow]" in in_progress_line
        assert "▶" in in_progress_line
        assert "[/yellow]" in in_progress_line

    def test_completed_todo_is_green_with_strikethrough(self, handler):
        """Test that completed todos use green color with strikethrough."""
        formatted = handler._format_todo_list_simple()
        completed_line = [line for line in formatted if "Completed task" in line][0]
        assert "[green]" in completed_line
        assert "✓" in completed_line
        assert "~~" in completed_line  # Strikethrough markup
        assert "[/green]" in completed_line

    def test_write_todos_output_has_symbols(self, handler):
        """Test that write_todos output includes status symbols (no Rich markup)."""
        result = handler.write_todos(["New task 1", "New task 2"])
        assert result["success"]

        output = result["output"]
        # Should have symbols for pending todos (no Rich markup - output goes to plain text)
        assert "○" in output
        # Should NOT have Rich markup tags
        assert "[cyan]" not in output
        assert "[/cyan]" not in output


class TestTodoHandlerUpdateComplete:
    """Test suite for update_todo and complete_todo with fuzzy ID matching."""

    @pytest.fixture
    def handler(self):
        """Create a TodoHandler with sample todos."""
        handler = TodoHandler()
        handler.create_todo(title="Implement basic level design with platforms and ground")
        handler.create_todo(title="Add enemy characters (Goombas, Koopas) with basic AI")
        return handler

    def test_update_todo_with_kebab_case_id(self, handler):
        """Test updating todo using kebab-case slug ID."""
        result = handler.update_todo(id="implement-basic-level", status="doing")
        assert result["success"]
        assert "Implement basic level design" in result["output"]

        # Verify the todo was actually updated
        _, todo = handler._find_todo("implement-basic-level")
        assert todo.status == "doing"

    def test_complete_todo_with_partial_match(self, handler):
        """Test completing todo using partial string match."""
        result = handler.complete_todo(id="enemy characters")
        assert result["success"]
        assert "Add enemy characters" in result["output"]

        # Verify the todo was actually completed
        _, todo = handler._find_todo("enemy characters")
        assert todo.status == "done"

    def test_update_nonexistent_todo_fails(self, handler):
        """Test that updating non-existent todo returns error."""
        result = handler.update_todo(id="nonexistent-task", status="doing")
        assert not result["success"]
        assert "not found" in result["error"]

    def test_complete_nonexistent_todo_fails(self, handler):
        """Test that completing non-existent todo returns error."""
        result = handler.complete_todo(id="nonexistent-task")
        assert not result["success"]
        assert "not found" in result["error"]


class TestTodoCompletionCounter:
    """Test suite for verifying todo completion counter logic."""

    def test_completion_counter_all_done(self):
        """Verify counter shows N/N when all todos are completed."""
        handler = TodoHandler()

        # Create 3 todos
        handler.write_todos(["Task 1", "Task 2", "Task 3"])

        # Mark all as done
        handler.update_todo("todo-1", status="completed")
        handler.update_todo("todo-2", status="completed")
        handler.update_todo("todo-3", status="completed")

        # Verify all have status="done"
        todos = list(handler._todos.values())
        assert len(todos) == 3
        assert all(t.status == "done" for t in todos)

        # Calculate what counter should show
        completed = len([t for t in todos if t.status == "done"])
        total = len(todos)
        assert completed == 3
        assert total == 3

    def test_spinner_callback_guard_no_active_todo(self):
        """Verify spinner callback would be guarded when no active todo exists."""
        handler = TodoHandler()
        handler.write_todos([{"content": "Task 1", "status": "completed"}])

        # All todos are done - spinner callback should be guarded
        todos = list(handler._todos.values())
        has_active = any(t.status == "doing" for t in todos)
        assert not has_active  # Guard would prevent spinner render

    def test_completion_counter_partial_done(self):
        """Verify counter shows correct count when some todos are completed."""
        handler = TodoHandler()

        # Create 4 todos
        handler.write_todos(["Task 1", "Task 2", "Task 3", "Task 4"])

        # Mark 2 as done
        handler.update_todo("todo-1", status="completed")
        handler.update_todo("todo-3", status="completed")

        todos = list(handler._todos.values())
        completed = len([t for t in todos if t.status == "done"])
        total = len(todos)
        assert completed == 2
        assert total == 4

    def test_completion_counter_with_active_todo(self):
        """Verify active todo detection works correctly."""
        handler = TodoHandler()

        # Create 3 todos
        handler.write_todos(["Task 1", "Task 2", "Task 3"])

        # Set one as in_progress
        handler.update_todo("todo-2", status="in_progress")

        todos = list(handler._todos.values())
        has_active = any(t.status == "doing" for t in todos)
        assert has_active  # Should have an active todo


class TestTodoHandlerStateTransitions:
    """Test complete state transition flows."""

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

    def test_complete_and_activate_next_no_pending(self):
        """Test complete_and_activate_next when no pending todos remain."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])
        handler.update_todo("todo-1", status="doing")

        result = handler.complete_and_activate_next("todo-1")

        assert result["success"]
        assert handler._todos["todo-1"].status == "done"
        assert "All todos completed" in result["output"]

    def test_direct_to_done_skipping_doing(self):
        """Test marking todo directly from pending to done."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])

        # Skip doing and go directly to done
        handler.complete_todo("todo-1")
        assert handler._todos["todo-1"].status == "done"

    def test_status_mapping_from_deep_agent_format(self):
        """Test that Deep Agent status formats are correctly mapped."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])

        # pending -> todo
        handler.update_todo("todo-1", status="pending")
        assert handler._todos["todo-1"].status == "todo"

        # in_progress -> doing
        handler.update_todo("todo-1", status="in_progress")
        assert handler._todos["todo-1"].status == "doing"

        # completed -> done
        handler.update_todo("todo-1", status="completed")
        assert handler._todos["todo-1"].status == "done"


class TestTodoHandlerWriteTodos:
    """Test write_todos behavior including status-only updates."""

    def test_write_todos_replaces_existing(self):
        """write_todos should replace existing todo list."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])

        assert len(handler._todos) == 2

        # Write new todos - should replace
        handler.write_todos(["New Task"])

        assert len(handler._todos) == 1
        assert handler._todos["todo-1"].title == "New Task"

    def test_write_todos_with_dict_format(self):
        """write_todos should accept dict format (Deep Agent)."""
        handler = TodoHandler()
        result = handler.write_todos([
            {"content": "Task 1", "status": "pending"},
            {"content": "Task 2", "status": "in_progress"},
            {"content": "Task 3", "status": "completed"},
        ])

        assert result["success"]
        assert handler._todos["todo-1"].status == "todo"
        assert handler._todos["todo-2"].status == "doing"
        assert handler._todos["todo-3"].status == "done"

    def test_write_todos_with_active_form(self):
        """write_todos should preserve activeForm."""
        handler = TodoHandler()
        result = handler.write_todos([
            {"content": "Run tests", "status": "in_progress", "activeForm": "Running tests"},
        ])

        assert result["success"]
        assert handler._todos["todo-1"].active_form == "Running tests"

    def test_status_only_update_detection(self):
        """write_todos should detect status-only updates."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])

        # Same content, different status
        result = handler.write_todos([
            {"content": "Task 1", "status": "in_progress"},
            {"content": "Task 2", "status": "pending"},
        ])

        assert result["success"]
        # Should have updated status without recreating
        assert handler._todos["todo-1"].status == "doing"


class TestTodoHandlerCompletionHelpers:
    """Test suite for completion enforcement helper methods."""

    def test_has_todos_empty(self):
        """has_todos returns False when no todos exist."""
        handler = TodoHandler()
        assert handler.has_todos() is False

    def test_has_todos_with_todos(self):
        """has_todos returns True when todos exist."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])
        assert handler.has_todos() is True

    def test_has_incomplete_todos_empty(self):
        """has_incomplete_todos returns False when no todos exist."""
        handler = TodoHandler()
        assert handler.has_incomplete_todos() is False

    def test_has_incomplete_todos_all_done(self):
        """has_incomplete_todos returns False when all todos are done."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])
        handler.complete_todo("todo-1")
        handler.complete_todo("todo-2")
        assert handler.has_incomplete_todos() is False

    def test_has_incomplete_todos_some_pending(self):
        """has_incomplete_todos returns True when some todos are pending."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])
        handler.complete_todo("todo-1")
        assert handler.has_incomplete_todos() is True

    def test_has_incomplete_todos_some_doing(self):
        """has_incomplete_todos returns True when some todos are in progress."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])
        handler.update_todo("todo-1", status="doing")
        handler.complete_todo("todo-2")
        assert handler.has_incomplete_todos() is True

    def test_get_incomplete_todos_empty(self):
        """get_incomplete_todos returns empty list when no todos exist."""
        handler = TodoHandler()
        assert handler.get_incomplete_todos() == []

    def test_get_incomplete_todos_all_done(self):
        """get_incomplete_todos returns empty list when all done."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2"])
        handler.complete_todo("todo-1")
        handler.complete_todo("todo-2")
        assert handler.get_incomplete_todos() == []

    def test_get_incomplete_todos_mixed_status(self):
        """get_incomplete_todos returns only incomplete todos."""
        handler = TodoHandler()
        handler.write_todos(["Task 1", "Task 2", "Task 3"])
        handler.update_todo("todo-1", status="doing")  # incomplete
        handler.complete_todo("todo-2")  # done
        # todo-3 remains pending (incomplete)

        incomplete = handler.get_incomplete_todos()
        assert len(incomplete) == 2
        titles = [t.title for t in incomplete]
        assert "Task 1" in titles
        assert "Task 3" in titles
        assert "Task 2" not in titles

    def test_get_incomplete_todos_returns_todoitem_objects(self):
        """get_incomplete_todos returns TodoItem objects."""
        handler = TodoHandler()
        handler.write_todos(["Task 1"])
        incomplete = handler.get_incomplete_todos()
        assert len(incomplete) == 1
        assert isinstance(incomplete[0], TodoItem)
        assert incomplete[0].title == "Task 1"
        assert incomplete[0].status == "todo"


class TestTodoMarkdownStripping:
    """Test suite for markdown stripping from todo titles."""

    def test_strip_bold(self):
        """Bold markers are removed."""
        handler = TodoHandler()
        handler.create_todo(title="**Configure Build Pipeline**: Adjust tsconfig")
        assert handler._todos["todo-1"].title == "Configure Build Pipeline: Adjust tsconfig"

    def test_strip_italic_asterisk(self):
        """Italic asterisk markers are removed."""
        handler = TodoHandler()
        handler.create_todo(title="*Important* task to do")
        assert handler._todos["todo-1"].title == "Important task to do"

    def test_strip_italic_underscore(self):
        """Italic underscore markers are removed."""
        handler = TodoHandler()
        handler.create_todo(title="An _important_ task")
        assert handler._todos["todo-1"].title == "An important task"

    def test_strip_inline_code(self):
        """Backtick markers are removed."""
        handler = TodoHandler()
        handler.create_todo(title="Fix `TypeError` in parser")
        assert handler._todos["todo-1"].title == "Fix TypeError in parser"

    def test_strip_strikethrough(self):
        """Strikethrough markers are removed."""
        handler = TodoHandler()
        handler.create_todo(title="Remove ~~deprecated~~ API calls")
        assert handler._todos["todo-1"].title == "Remove deprecated API calls"

    def test_strip_link(self):
        """Markdown links become plain text."""
        handler = TodoHandler()
        handler.create_todo(title="See [docs](https://example.com) for details")
        assert handler._todos["todo-1"].title == "See docs for details"

    def test_strip_heading(self):
        """Heading markers are removed."""
        handler = TodoHandler()
        handler.create_todo(title="## Setup environment")
        assert handler._todos["todo-1"].title == "Setup environment"

    def test_strip_mixed_markdown(self):
        """Multiple markdown formats are stripped together."""
        handler = TodoHandler()
        handler.create_todo(title="**Fix** the `bug` in *parser* module")
        assert handler._todos["todo-1"].title == "Fix the bug in parser module"

    def test_plain_text_passthrough(self):
        """Plain text passes through unchanged."""
        handler = TodoHandler()
        handler.create_todo(title="Fix authentication bug in login flow")
        assert handler._todos["todo-1"].title == "Fix authentication bug in login flow"

    def test_write_todos_dict_format_strips_markdown(self):
        """write_todos with dict format strips markdown from content."""
        handler = TodoHandler()
        handler.write_todos([
            {"content": "**Step 1**: Initialize project", "status": "pending"},
            {"content": "Fix `config` issue", "status": "pending"},
        ])
        assert handler._todos["todo-1"].title == "Step 1: Initialize project"
        assert handler._todos["todo-2"].title == "Fix config issue"

    def test_update_todo_title_strips_markdown(self):
        """update_todo strips markdown when title is changed."""
        handler = TodoHandler()
        handler.create_todo(title="Original task")
        handler.update_todo("todo-1", title="**Updated** task with `code`")
        assert handler._todos["todo-1"].title == "Updated task with code"

    def test_strip_bold_underscore(self):
        """Bold underscore markers are removed."""
        handler = TodoHandler()
        handler.create_todo(title="__Bold underscore__ text")
        assert handler._todos["todo-1"].title == "Bold underscore text"

    def test_underscore_in_identifiers_preserved(self):
        """Underscores within words (like variable names) are preserved."""
        handler = TodoHandler()
        handler.create_todo(title="Fix my_variable_name in code")
        assert handler._todos["todo-1"].title == "Fix my_variable_name in code"

    def test_create_todo_strips_active_form(self):
        """create_todo strips markdown from active_form."""
        handler = TodoHandler()
        handler.create_todo(
            title="Extend PreloadScene",
            active_form="**extending PreloadScene**: preload tilemap JSON",
        )
        assert handler._todos["todo-1"].active_form == "extending PreloadScene: preload tilemap JSON"

    def test_write_todos_dict_strips_active_form(self):
        """write_todos with dict format strips markdown from activeForm."""
        handler = TodoHandler()
        handler.write_todos([
            {
                "content": "Set up project",
                "status": "pending",
                "activeForm": "**Setting up** `project`",
            },
        ])
        assert handler._todos["todo-1"].active_form == "Setting up project"

    def test_update_todo_strips_active_form(self):
        """update_todo strips markdown from active_form."""
        handler = TodoHandler()
        handler.create_todo(title="Task", active_form="Working")
        handler.update_todo("todo-1", active_form="**Fixing** the `bug`")
        assert handler._todos["todo-1"].active_form == "Fixing the bug"
