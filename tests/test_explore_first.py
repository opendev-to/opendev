"""Tests for explore-first enforcement before subagent spawning."""

import json
from unittest.mock import MagicMock, patch

import pytest

from opendev.repl.react_executor import ReactExecutor, IterationContext
from opendev.repl.react_executor.executor import LoopAction


def _make_tool_call(name: str, args: dict | None = None, call_id: str | None = None) -> dict:
    return {
        "id": call_id or f"call_{name}",
        "function": {
            "name": name,
            "arguments": json.dumps(args or {}),
        },
    }


@pytest.fixture
def executor():
    console = MagicMock()
    session_manager = MagicMock()
    config = MagicMock()
    config.auto_save_interval = 5
    llm_caller = MagicMock()
    tool_executor = MagicMock()
    tool_executor.mode_manager = MagicMock()
    return ReactExecutor(session_manager, config, mode_manager=tool_executor.mode_manager, console=console, llm_caller=llm_caller, tool_executor=tool_executor)


@pytest.fixture
def ctx():
    return IterationContext(
        query="test",
        messages=[],
        agent=MagicMock(),
        tool_registry=MagicMock(),
        approval_manager=MagicMock(),
        undo_manager=MagicMock(),
        ui_callback=MagicMock(),
    )


class TestExploreFirstEnforcement:
    """Test that non-exempt subagents are blocked until Code-Explorer has run."""

    def test_blocks_planner_before_exploration(self, executor, ctx):
        """Planner spawn should be blocked when has_explored is False."""
        tool_calls = [
            _make_tool_call("spawn_subagent", {"subagent_type": "Planner"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None):
            result = executor._process_tool_calls(ctx, tool_calls, "", None)
        assert result == LoopAction.CONTINUE
        assert not ctx.has_explored
        # Should have injected the nudge as tool result
        tool_msgs = [m for m in ctx.messages if m.get("role") == "tool"]
        assert len(tool_msgs) == 1
        assert "explore" in tool_msgs[0]["content"].lower()

    def test_allows_code_explorer_without_prior_exploration(self, executor, ctx):
        """Code-Explorer should always be allowed and should set has_explored."""
        tool_calls = [
            _make_tool_call("spawn_subagent", {"subagent_type": "Code-Explorer"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None), \
             patch.object(executor, "_execute_single_tool") as mock_exec, \
             patch.object(executor, "_add_tool_result_to_history"), \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False), \
             patch.object(executor, "_push_context_usage"):
            mock_exec.return_value = {"success": True, "output": "explored"}
            executor._process_tool_calls(ctx, tool_calls, "", None)
        assert ctx.has_explored is True

    def test_allows_ask_user_without_prior_exploration(self, executor, ctx):
        """ask-user should always be allowed (exempt)."""
        tool_calls = [
            _make_tool_call("spawn_subagent", {"subagent_type": "ask-user"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None), \
             patch.object(executor, "_execute_single_tool") as mock_exec, \
             patch.object(executor, "_add_tool_result_to_history"), \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False), \
             patch.object(executor, "_push_context_usage"):
            mock_exec.return_value = {"success": True, "output": "asked"}
            result = executor._process_tool_calls(ctx, tool_calls, "", None)
        assert result == LoopAction.CONTINUE
        # ask-user doesn't set has_explored
        assert ctx.has_explored is False

    def test_allows_planner_after_exploration(self, executor, ctx):
        """Planner should proceed normally after Code-Explorer has run."""
        ctx.has_explored = True
        tool_calls = [
            _make_tool_call("spawn_subagent", {"subagent_type": "Planner"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None), \
             patch.object(executor, "_execute_single_tool") as mock_exec, \
             patch.object(executor, "_add_tool_result_to_history"), \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False), \
             patch.object(executor, "_push_context_usage"):
            mock_exec.return_value = {"success": True, "output": "planned"}
            result = executor._process_tool_calls(ctx, tool_calls, "", None)
        assert result == LoopAction.CONTINUE
        mock_exec.assert_called_once()

    def test_blocks_web_generator_before_exploration(self, executor, ctx):
        """Web-Generator should be blocked before exploration."""
        tool_calls = [
            _make_tool_call("spawn_subagent", {"subagent_type": "Web-Generator"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None):
            result = executor._process_tool_calls(ctx, tool_calls, "", None)
        assert result == LoopAction.CONTINUE
        tool_msgs = [m for m in ctx.messages if m.get("role") == "tool"]
        assert "explore" in tool_msgs[0]["content"].lower()

    def test_blocks_batch_spawn_fills_all_tool_results(self, executor, ctx):
        """When blocking a batch of spawns, all tool calls get synthetic results."""
        tool_calls = [
            _make_tool_call("spawn_subagent", {"subagent_type": "Planner"}, "c1"),
            _make_tool_call("spawn_subagent", {"subagent_type": "Web-Generator"}, "c2"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None):
            result = executor._process_tool_calls(ctx, tool_calls, "", None)
        assert result == LoopAction.CONTINUE
        tool_msgs = [m for m in ctx.messages if m.get("role") == "tool"]
        # Should have results for both tool calls (assistant msg + 2 tool results)
        assert len(tool_msgs) == 2
        tool_call_ids = {m["tool_call_id"] for m in tool_msgs}
        assert tool_call_ids == {"c1", "c2"}

    def test_non_subagent_tools_unaffected(self, executor, ctx):
        """Regular tools like read_file should not be affected by explore-first."""
        tool_calls = [
            _make_tool_call("read_file", {"path": "foo.py"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None), \
             patch.object(executor, "_execute_single_tool") as mock_exec, \
             patch.object(executor, "_add_tool_result_to_history"), \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False), \
             patch.object(executor, "_push_context_usage"):
            mock_exec.return_value = {"success": True, "output": "content"}
            executor._process_tool_calls(ctx, tool_calls, "", None)
        mock_exec.assert_called_once()


class TestExploreFirstReadBlocking:
    """Test that excessive exploration reads are blocked before Code-Explorer runs."""

    def test_block_reads_after_3_without_exploration(self, executor, ctx):
        """When has_explored=False and 3rd consecutive read batch arrives, block with nudge."""
        ctx.has_explored = False
        ctx.consecutive_reads = 2  # Already had 2 consecutive read batches

        tool_calls = [
            _make_tool_call("read_file", {"path": "foo.py"}, "c1"),
            _make_tool_call("search", {"query": "bar"}, "c2"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None):
            result = executor._process_tool_calls(ctx, tool_calls, "", None)

        assert result == LoopAction.CONTINUE
        # Reads should have been blocked (no execution)
        tool_msgs = [m for m in ctx.messages if m.get("role") == "tool"]
        assert len(tool_msgs) == 2
        # All tool results should contain the delegate nudge
        for msg in tool_msgs:
            assert "Code-Explorer" in msg["content"]
        # Counter should be reset
        assert ctx.consecutive_reads == 0
        assert ctx.skip_next_thinking is True

    def test_reads_allowed_after_exploration(self, executor, ctx):
        """When has_explored=True, reads proceed normally even at 3+ consecutive."""
        ctx.has_explored = True
        ctx.consecutive_reads = 2  # Already had 2 consecutive read batches

        tool_calls = [
            _make_tool_call("read_file", {"path": "foo.py"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None), \
             patch.object(executor, "_execute_single_tool") as mock_exec, \
             patch.object(executor, "_add_tool_result_to_history"), \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False), \
             patch.object(executor, "_push_context_usage"):
            mock_exec.return_value = {"success": True, "output": "content"}
            result = executor._process_tool_calls(ctx, tool_calls, "", None)

        assert result == LoopAction.CONTINUE
        mock_exec.assert_called_once()

    def test_reads_under_threshold_allowed(self, executor, ctx):
        """1-2 consecutive reads proceed normally even without exploration."""
        ctx.has_explored = False
        ctx.consecutive_reads = 0  # First read batch

        tool_calls = [
            _make_tool_call("read_file", {"path": "foo.py"}, "c1"),
        ]
        with patch.object(executor, "_display_message"), \
             patch.object(executor, "_detect_doom_loop", return_value=None), \
             patch.object(executor, "_execute_single_tool") as mock_exec, \
             patch.object(executor, "_add_tool_result_to_history"), \
             patch.object(executor, "_persist_step"), \
             patch.object(executor, "_should_nudge_agent", return_value=False), \
             patch.object(executor, "_push_context_usage"):
            mock_exec.return_value = {"success": True, "output": "content"}
            result = executor._process_tool_calls(ctx, tool_calls, "", None)

        assert result == LoopAction.CONTINUE
        mock_exec.assert_called_once()
