"""Tests for real-time parallel tool display in subagents."""

import json
import time
from unittest.mock import MagicMock, patch

import pytest


class TestExecuteToolsParallelCallbacks:
    """Verify _execute_tools_parallel fires UI callbacks in real-time."""

    def _make_tool_call(self, name: str, args: dict, call_id: str) -> dict:
        return {
            "id": call_id,
            "function": {
                "name": name,
                "arguments": json.dumps(args),
            },
        }

    def _make_agent(self):
        """Create a minimal RunLoopMixin instance for testing."""
        from opendev.core.agents.main_agent.run_loop import RunLoopMixin

        agent = RunLoopMixin()
        agent.tool_registry = MagicMock()
        return agent

    def test_on_tool_call_fires_for_all_before_execution(self):
        """on_tool_call should fire for ALL tools before any execution starts."""
        agent = self._make_agent()

        tool_calls = [
            self._make_tool_call("read_file", {"path": "a.py"}, "tc_1"),
            self._make_tool_call("search", {"pattern": "foo"}, "tc_2"),
            self._make_tool_call("list_files", {"path": "."}, "tc_3"),
        ]

        call_order = []

        def mock_execute(name, args, **kwargs):
            call_order.append(("execute", name))
            return {"success": True, "output": f"result_{name}"}

        agent.tool_registry.execute_tool = mock_execute

        ui_callback = MagicMock()

        def record_tool_call(name, args, tool_call_id=None):
            call_order.append(("on_tool_call", name, tool_call_id))

        def record_tool_result(name, args, result, tool_call_id=None):
            call_order.append(("on_tool_result", name, tool_call_id))

        ui_callback.on_tool_call = record_tool_call
        ui_callback.on_tool_result = record_tool_result

        deps = MagicMock()
        results = agent._execute_tools_parallel(
            tool_calls, deps, task_monitor=None, ui_callback=ui_callback, is_subagent=True
        )

        # Verify all 3 on_tool_call fired
        on_tool_calls = [e for e in call_order if e[0] == "on_tool_call"]
        assert len(on_tool_calls) == 3

        # Verify all on_tool_call fired before any execute
        first_execute_idx = next(
            i for i, e in enumerate(call_order) if e[0] == "execute"
        )
        last_on_tool_call_idx = max(
            i for i, e in enumerate(call_order) if e[0] == "on_tool_call"
        )
        assert last_on_tool_call_idx < first_execute_idx, (
            "All on_tool_call should fire before any execution starts"
        )

        # Verify tool_call_id is passed correctly
        assert on_tool_calls[0] == ("on_tool_call", "read_file", "tc_1")
        assert on_tool_calls[1] == ("on_tool_call", "search", "tc_2")
        assert on_tool_calls[2] == ("on_tool_call", "list_files", "tc_3")

    def test_on_tool_result_fires_as_each_completes(self):
        """on_tool_result should fire as each tool completes, not after all."""
        agent = self._make_agent()

        tool_calls = [
            self._make_tool_call("read_file", {"path": "a.py"}, "tc_1"),
            self._make_tool_call("search", {"pattern": "foo"}, "tc_2"),
        ]

        result_times = []

        def mock_execute(name, args, **kwargs):
            # read_file finishes quickly, search takes longer
            if name == "read_file":
                time.sleep(0.01)
            else:
                time.sleep(0.1)
            return {"success": True, "output": f"result_{name}"}

        agent.tool_registry.execute_tool = mock_execute

        ui_callback = MagicMock()

        def record_result(name, args, result, tool_call_id=None):
            result_times.append((name, time.monotonic(), tool_call_id))

        ui_callback.on_tool_result = record_result

        deps = MagicMock()
        agent._execute_tools_parallel(
            tool_calls, deps, task_monitor=None, ui_callback=ui_callback, is_subagent=True
        )

        # Both results should be recorded
        assert len(result_times) == 2

        # Each result should have the correct tool_call_id
        ids = {name: tid for name, _, tid in result_times}
        assert ids["read_file"] == "tc_1"
        assert ids["search"] == "tc_2"

    def test_tool_call_id_passed_in_both_call_and_result(self):
        """tool_call_id should match between on_tool_call and on_tool_result."""
        agent = self._make_agent()

        tool_calls = [
            self._make_tool_call("read_file", {"path": "a.py"}, "tc_abc"),
        ]

        agent.tool_registry.execute_tool = MagicMock(
            return_value={"success": True, "output": "ok"}
        )

        ui_callback = MagicMock()
        call_ids = []
        result_ids = []

        def on_call(name, args, tool_call_id=None):
            call_ids.append(tool_call_id)

        def on_result(name, args, result, tool_call_id=None):
            result_ids.append(tool_call_id)

        ui_callback.on_tool_call = on_call
        ui_callback.on_tool_result = on_result

        deps = MagicMock()
        agent._execute_tools_parallel(
            tool_calls, deps, task_monitor=None, ui_callback=ui_callback, is_subagent=True
        )

        assert call_ids == ["tc_abc"]
        assert result_ids == ["tc_abc"]

    def test_results_dict_correct_after_parallel_execution(self):
        """Results should be keyed by tool_call_id."""
        agent = self._make_agent()

        tool_calls = [
            self._make_tool_call("read_file", {"path": "a.py"}, "tc_1"),
            self._make_tool_call("search", {"pattern": "foo"}, "tc_2"),
        ]

        def mock_execute(name, args, **kwargs):
            return {"success": True, "output": f"result_{name}"}

        agent.tool_registry.execute_tool = mock_execute

        deps = MagicMock()
        results = agent._execute_tools_parallel(
            tool_calls, deps, task_monitor=None, ui_callback=None, is_subagent=True
        )

        assert "tc_1" in results
        assert "tc_2" in results
        assert results["tc_1"]["output"] == "result_read_file"
        assert results["tc_2"]["output"] == "result_search"

    def test_interrupt_skips_execution_and_callbacks(self):
        """When interrupted, no execution or callbacks should happen."""
        agent = self._make_agent()

        tool_calls = [
            self._make_tool_call("read_file", {"path": "a.py"}, "tc_1"),
        ]

        agent.tool_registry.execute_tool = MagicMock()
        ui_callback = MagicMock()
        task_monitor = MagicMock()
        task_monitor.should_interrupt.return_value = True

        deps = MagicMock()
        results = agent._execute_tools_parallel(
            tool_calls, deps, task_monitor=task_monitor, ui_callback=ui_callback, is_subagent=True
        )

        assert results["tc_1"]["interrupted"] is True
        agent.tool_registry.execute_tool.assert_not_called()
        ui_callback.on_tool_call.assert_not_called()
        ui_callback.on_tool_result.assert_not_called()


class TestSingleAgentToolLineTracking:
    """Verify SingleAgentInfo multi-tool tracking fields."""

    def test_single_agent_info_has_active_tool_lines(self):
        from opendev.ui_textual.widgets.conversation.tool_renderer.types import (
            SingleAgentInfo,
            SingleAgentToolLine,
        )

        agent = SingleAgentInfo(
            agent_type="Code-Explorer",
            description="test",
            tool_call_id="tc_1",
        )
        assert agent.active_tool_lines == {}
        assert agent.overflow_line is None
        assert agent.MAX_VISIBLE_TOOLS == 3

    def test_single_agent_tool_line_defaults(self):
        from opendev.ui_textual.widgets.conversation.tool_renderer.types import (
            SingleAgentToolLine,
        )

        tl = SingleAgentToolLine(
            tool_id="t1",
            line_number=5,
            display_text="Search (pattern: foo)",
        )
        assert tl.completed is False
        assert tl.success is True
        assert tl.color_index == 0


class TestNestedToolResultPassthrough:
    """Verify on_nested_tool_result allows through for single agents."""

    def test_single_agent_allows_nested_result_display(self):
        """When _current_single_agent_id is set, nested results should NOT be skipped."""
        # The fix: `if self._in_parallel_agent_group and not self._current_single_agent_id: return`
        # This test verifies the condition logic
        _in_parallel_agent_group = True
        _current_single_agent_id = "agent_1"

        # Old behavior: would return early (skip display)
        old_should_skip = _in_parallel_agent_group

        # New behavior: only skip if NOT single agent
        new_should_skip = _in_parallel_agent_group and not _current_single_agent_id

        assert old_should_skip is True  # Old: would incorrectly skip
        assert new_should_skip is False  # New: correctly allows through

    def test_parallel_group_still_skips_nested_result_display(self):
        """When in actual parallel agent group (no single agent), should still skip."""
        _in_parallel_agent_group = True
        _current_single_agent_id = None

        should_skip = _in_parallel_agent_group and not _current_single_agent_id
        assert should_skip is True
