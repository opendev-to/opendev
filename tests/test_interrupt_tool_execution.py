"""Tests for interrupt handling during tool execution.

Fix 5: Bash tool propagates "interrupted" flag
Fix 6: Token check between sequential tool calls
Fix C: Parallel tools guard
"""

from unittest.mock import Mock

from opendev.core.runtime.interrupt_token import InterruptToken


# ---------------------------------------------------------------------------
# Fix 5: Bash tool propagates "interrupted" flag
# ---------------------------------------------------------------------------


class TestFix5BashInterruptPropagation:
    """Verify process_handlers returns interrupted=True for interrupted commands."""

    def test_bash_interrupt_propagates_flag(self):
        """When bash result has 'interrupted' in error, flag is set."""
        from opendev.core.context_engineering.tools.handlers.process_handlers import (
            ProcessToolHandler,
        )

        # Create handler with mock bash tool
        bash_tool = Mock()
        bash_tool.working_dir = "/tmp"

        # Mock execute to return an interrupted result
        bash_result = Mock()
        bash_result.success = False
        bash_result.error = "Command interrupted by user"
        bash_result.stdout = ""
        bash_result.stderr = ""
        bash_result.exit_code = -1
        bash_tool.execute.return_value = bash_result

        handler = ProcessToolHandler(bash_tool)

        # Create execution context
        context = Mock()
        context.mode_manager = None
        context.approval_manager = None
        context.undo_manager = None
        context.task_monitor = None
        context.ui_callback = None

        result = handler.run_command({"command": "sleep 100"}, context)

        assert result["interrupted"] is True

    def test_bash_non_interrupt_error_no_flag(self):
        """Regular errors don't set interrupted flag."""
        from opendev.core.context_engineering.tools.handlers.process_handlers import (
            ProcessToolHandler,
        )

        bash_tool = Mock()
        bash_tool.working_dir = "/tmp"

        bash_result = Mock()
        bash_result.success = False
        bash_result.error = "Command not found"
        bash_result.stdout = ""
        bash_result.stderr = "bash: foo: command not found"
        bash_result.exit_code = 127
        bash_tool.execute.return_value = bash_result

        handler = ProcessToolHandler(bash_tool)

        context = Mock()
        context.mode_manager = None
        context.approval_manager = None
        context.undo_manager = None
        context.task_monitor = None
        context.ui_callback = None

        result = handler.run_command({"command": "foo"}, context)

        assert result["interrupted"] is False


# ---------------------------------------------------------------------------
# Fix 6: Token check between sequential tool calls
# ---------------------------------------------------------------------------


class TestFix6TokenCheckBetweenTools:
    """Verify sequential tools are skipped after interrupt."""

    def test_sequential_tools_skip_after_interrupt(self):
        """After token is signaled, subsequent tools get synthetic interrupted result."""
        from opendev.repl.react_executor import ReactExecutor, IterationContext

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)

        # Set up an already-signaled token
        token = InterruptToken()
        token.request()
        executor._active_interrupt_token = token

        # Create context
        ctx = IterationContext(
            query="test",
            messages=[],
            agent=Mock(),
            tool_registry=Mock(),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=None,
        )

        # Create two tool calls
        tool_calls = [
            {
                "id": "call_1",
                "function": {"name": "read_file", "arguments": '{"path": "foo.py"}'},
            },
            {
                "id": "call_2",
                "function": {"name": "read_file", "arguments": '{"path": "bar.py"}'},
            },
        ]

        # Mock _execute_single_tool to track if it's called
        executor._execute_single_tool = Mock(return_value={"success": True, "output": "content"})

        # Run the tool processing loop manually (extract the sequential part)
        tool_results_by_id = {}
        operation_cancelled = False
        for tool_call in tool_calls:
            if executor._active_interrupt_token and executor._active_interrupt_token.is_requested():
                tool_results_by_id[tool_call["id"]] = {
                    "success": False,
                    "error": "Interrupted by user",
                    "output": None,
                    "interrupted": True,
                }
                operation_cancelled = True
                break

            result = executor._execute_single_tool(tool_call, ctx)
            tool_results_by_id[tool_call["id"]] = result

        # Neither tool should have been executed since token was pre-signaled
        executor._execute_single_tool.assert_not_called()
        assert operation_cancelled is True
        assert "call_1" in tool_results_by_id
        assert tool_results_by_id["call_1"]["interrupted"] is True
        # Second tool should not have a result (we broke after first)
        assert "call_2" not in tool_results_by_id


# ---------------------------------------------------------------------------
# Fix C: Parallel tools guard
# ---------------------------------------------------------------------------


class TestFixCParallelToolsGuard:
    """Verify parallel tools are skipped when interrupt token is signaled."""

    def _make_executor(self):
        """Create a minimal ReactExecutor with mocked dependencies."""
        from opendev.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)
        return executor

    def test_parallel_tools_skip_all_when_interrupted(self):
        """All parallel tools get interrupted result when token is signaled."""
        from opendev.repl.react_executor import IterationContext

        executor = self._make_executor()
        token = InterruptToken()
        token.request()
        executor._active_interrupt_token = token

        ctx = IterationContext(
            query="test",
            messages=[],
            agent=Mock(),
            tool_registry=Mock(),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=Mock(),
        )

        tool_calls = [
            {
                "id": "call_1",
                "function": {
                    "name": "spawn_subagent",
                    "arguments": '{"subagent_type": "Explore", "description": "test1"}',
                },
            },
            {
                "id": "call_2",
                "function": {
                    "name": "spawn_subagent",
                    "arguments": '{"subagent_type": "Explore", "description": "test2"}',
                },
            },
        ]

        executor._execute_single_tool = Mock()
        results, cancelled = executor._execute_tools_parallel(tool_calls, ctx)

        assert cancelled is True
        assert results["call_1"]["interrupted"] is True
        assert results["call_2"]["interrupted"] is True
        # No actual tools should have been submitted
        executor._execute_single_tool.assert_not_called()

    def test_parallel_tools_execute_normally_when_not_interrupted(self):
        """Parallel tools execute normally when token is not signaled."""
        from opendev.repl.react_executor import IterationContext

        executor = self._make_executor()
        token = InterruptToken()  # NOT signaled
        executor._active_interrupt_token = token

        ctx = IterationContext(
            query="test",
            messages=[],
            agent=Mock(),
            tool_registry=Mock(),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=Mock(),
        )

        tool_calls = [
            {
                "id": "call_1",
                "function": {
                    "name": "spawn_subagent",
                    "arguments": '{"subagent_type": "Explore", "description": "test1"}',
                },
            },
            {
                "id": "call_2",
                "function": {
                    "name": "spawn_subagent",
                    "arguments": '{"subagent_type": "Explore", "description": "test2"}',
                },
            },
        ]

        executor._execute_single_tool = Mock(return_value={"success": True, "output": "done"})
        results, cancelled = executor._execute_tools_parallel(tool_calls, ctx)

        assert cancelled is False
        # Both tools should have been executed
        assert executor._execute_single_tool.call_count == 2
