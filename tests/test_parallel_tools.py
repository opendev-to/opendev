"""Tests for silent parallel execution of read-only tools."""

import json
from unittest.mock import MagicMock, patch

import pytest

from opendev.repl.react_executor import ReactExecutor


class TestParallelizableToolsSet:
    """Verify the PARALLELIZABLE_TOOLS constant is correct."""

    def test_contains_read_only_tools(self):
        """Core read-only tools should be in the set."""
        expected = {
            "read_file", "list_files", "search",
            "fetch_url", "web_search",
            "list_todos", "search_tools",
            "find_symbol", "find_referencing_symbols",
        }
        assert expected.issubset(ReactExecutor.PARALLELIZABLE_TOOLS)

    def test_excludes_mutation_tools(self):
        """Write/mutation tools must NOT be in the set."""
        excluded = {
            "write_file", "edit_file", "run_command",
            "spawn_subagent", "ask_user", "task_complete",
        }
        assert excluded.isdisjoint(ReactExecutor.PARALLELIZABLE_TOOLS)

    def test_is_frozenset(self):
        """Should be immutable to prevent accidental mutation."""
        assert isinstance(ReactExecutor.PARALLELIZABLE_TOOLS, frozenset)


def _make_tool_call(name: str, args: dict | None = None, call_id: str | None = None) -> dict:
    """Helper to create a tool call dict."""
    return {
        "id": call_id or f"call_{name}",
        "function": {
            "name": name,
            "arguments": json.dumps(args or {}),
        },
    }


class TestParallelismDecision:
    """Test that _process_tool_calls picks the right execution path."""

    @pytest.fixture
    def executor(self):
        """Create a ReactExecutor with mocked dependencies."""
        console = MagicMock()
        session_manager = MagicMock()
        config = MagicMock()
        config.auto_save_interval = 5
        llm_caller = MagicMock()
        tool_executor = MagicMock()
        tool_executor.mode_manager = MagicMock()
        return ReactExecutor(session_manager, config, mode_manager=tool_executor.mode_manager, console=console, llm_caller=llm_caller, tool_executor=tool_executor)

    @pytest.fixture
    def ctx(self):
        """Create a minimal IterationContext."""
        from opendev.repl.react_executor import IterationContext
        return IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=MagicMock(),
        )

    def test_multiple_read_files_trigger_parallel(self, executor, ctx):
        """Two+ read_file calls should route to _execute_tools_parallel."""
        tool_calls = [
            _make_tool_call("read_file", {"path": "a.py"}, "c1"),
            _make_tool_call("read_file", {"path": "b.py"}, "c2"),
        ]
        with patch.object(executor, "_execute_tools_parallel") as mock_parallel, \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False):
            mock_parallel.return_value = (
                {"c1": {"success": True, "output": "a"}, "c2": {"success": True, "output": "b"}},
                False,
            )
            executor._process_tool_calls(ctx, tool_calls, "", None)
            mock_parallel.assert_called_once_with(tool_calls, ctx)

    def test_mixed_read_write_stays_sequential(self, executor, ctx):
        """Mixed read + write should NOT trigger parallel."""
        tool_calls = [
            _make_tool_call("read_file", {"path": "a.py"}, "c1"),
            _make_tool_call("write_file", {"path": "b.py", "content": "x"}, "c2"),
        ]
        with patch.object(executor, "_execute_tools_parallel") as mock_parallel, \
             patch.object(executor, "_execute_single_tool") as mock_single, \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False):
            mock_single.return_value = {"success": True, "output": "ok"}
            executor._process_tool_calls(ctx, tool_calls, "", None)
            mock_parallel.assert_not_called()
            assert mock_single.call_count == 2

    def test_single_read_stays_sequential(self, executor, ctx):
        """A single read_file should NOT trigger parallel (len > 1 guard)."""
        tool_calls = [
            _make_tool_call("read_file", {"path": "a.py"}, "c1"),
        ]
        with patch.object(executor, "_execute_tools_parallel") as mock_parallel, \
             patch.object(executor, "_execute_single_tool") as mock_single, \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False):
            mock_single.return_value = {"success": True, "output": "ok"}
            executor._process_tool_calls(ctx, tool_calls, "", None)
            mock_parallel.assert_not_called()

    def test_spawn_subagent_uses_parallel(self, executor, ctx):
        """Multiple spawn_subagent still uses the parallel path."""
        ctx.has_explored = True  # Skip explore-first enforcement
        tool_calls = [
            _make_tool_call("spawn_subagent", {"subagent_type": "explore"}, "c1"),
            _make_tool_call("spawn_subagent", {"subagent_type": "explore"}, "c2"),
        ]
        with patch.object(executor, "_execute_tools_parallel") as mock_parallel, \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False):
            mock_parallel.return_value = (
                {"c1": {"success": True, "output": ""}, "c2": {"success": True, "output": ""}},
                False,
            )
            executor._process_tool_calls(ctx, tool_calls, "", None)
            mock_parallel.assert_called_once()

    def test_heterogeneous_parallelizable_tools(self, executor, ctx):
        """Different parallelizable tools (search + list_files) should trigger parallel."""
        tool_calls = [
            _make_tool_call("search", {"query": "foo"}, "c1"),
            _make_tool_call("list_files", {"path": "."}, "c2"),
            _make_tool_call("read_file", {"path": "x.py"}, "c3"),
        ]
        with patch.object(executor, "_execute_tools_parallel") as mock_parallel, \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False):
            mock_parallel.return_value = (
                {
                    "c1": {"success": True, "output": ""},
                    "c2": {"success": True, "output": ""},
                    "c3": {"success": True, "output": ""},
                },
                False,
            )
            executor._process_tool_calls(ctx, tool_calls, "", None)
            mock_parallel.assert_called_once()


class TestSilentParallelExecution:
    """Test the silent parallel branch inside _execute_tools_parallel."""

    @pytest.fixture
    def executor(self):
        console = MagicMock()
        session_manager = MagicMock()
        config = MagicMock()
        config.auto_save_interval = 5
        llm_caller = MagicMock()
        tool_executor = MagicMock()
        tool_executor.mode_manager = MagicMock()
        return ReactExecutor(session_manager, config, mode_manager=tool_executor.mode_manager, console=console, llm_caller=llm_caller, tool_executor=tool_executor)

    @pytest.fixture
    def ctx(self):
        from opendev.repl.react_executor import IterationContext
        return IterationContext(
            query="test",
            messages=[],
            agent=MagicMock(),
            tool_registry=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
            ui_callback=MagicMock(),
        )

    def test_silent_parallel_replays_in_order(self, executor, ctx):
        """Results should be replayed via on_tool_call/on_tool_result in original order."""
        tool_calls = [
            _make_tool_call("read_file", {"path": "a.py"}, "c1"),
            _make_tool_call("read_file", {"path": "b.py"}, "c2"),
        ]

        def quiet_side_effect(tc, context):
            return {"success": True, "output": f"content of {tc['id']}"}

        with patch.object(executor, "_execute_tool_quietly", side_effect=quiet_side_effect):
            results, cancelled = executor._execute_tools_parallel(tool_calls, ctx)

        assert not cancelled
        assert results["c1"]["success"] is True
        assert results["c2"]["success"] is True

        # Verify on_tool_call was called in original order (c1 then c2)
        call_args_list = ctx.ui_callback.on_tool_call.call_args_list
        assert len(call_args_list) == 2
        assert call_args_list[0].args[0] == "read_file"
        assert call_args_list[1].args[0] == "read_file"

    def test_silent_parallel_handles_exception(self, executor, ctx):
        """Exceptions in quiet execution should be caught and converted to error dicts."""
        tool_calls = [
            _make_tool_call("read_file", {"path": "a.py"}, "c1"),
            _make_tool_call("search", {"query": "foo"}, "c2"),
        ]

        def quiet_side_effect(tc, context):
            if tc["id"] == "c2":
                raise RuntimeError("search failed")
            return {"success": True, "output": "content"}

        with patch.object(executor, "_execute_tool_quietly", side_effect=quiet_side_effect):
            results, cancelled = executor._execute_tools_parallel(tool_calls, ctx)

        assert results["c1"]["success"] is True
        assert results["c2"]["success"] is False
        assert "search failed" in results["c2"]["error"]
        assert not cancelled

    def test_silent_parallel_interrupt(self, executor, ctx):
        """Interrupted tools should set operation_cancelled."""
        tool_calls = [
            _make_tool_call("read_file", {"path": "a.py"}, "c1"),
            _make_tool_call("read_file", {"path": "b.py"}, "c2"),
        ]

        def quiet_side_effect(tc, context):
            if tc["id"] == "c2":
                return {"success": False, "interrupted": True, "error": "Interrupted"}
            return {"success": True, "output": "ok"}

        with patch.object(executor, "_execute_tool_quietly", side_effect=quiet_side_effect):
            results, cancelled = executor._execute_tools_parallel(tool_calls, ctx)

        assert cancelled is True
