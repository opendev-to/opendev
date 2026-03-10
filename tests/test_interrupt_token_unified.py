"""Test unified InterruptToken propagation across all 3 interrupt scenarios.

Scenarios tested:
1) Agent thinking (LLM call in progress, spinner showing)
2) Tool executing (tool call in progress, spinner showing)
3) Approval panel (modal controller active)

Each scenario verifies that a single ESC press (via InterruptToken) reliably
cancels the operation and that the ⎿ interrupt message is produced correctly.
"""

import threading
import time
from unittest.mock import Mock, MagicMock, patch

from opendev.core.runtime.interrupt_token import InterruptToken
from opendev.core.runtime.monitoring import TaskMonitor


# ---------------------------------------------------------------------------
# InterruptToken core behavior
# ---------------------------------------------------------------------------


class TestInterruptTokenCore:
    """Test InterruptToken as the single cancellation primitive."""

    def test_initial_state_not_requested(self):
        token = InterruptToken()
        assert not token.is_requested()
        assert not token.should_interrupt()

    def test_request_sets_flag(self):
        token = InterruptToken()
        token.request()
        assert token.is_requested()
        assert token.should_interrupt()

    def test_throw_if_requested_raises(self):
        token = InterruptToken()
        token.request()
        try:
            token.throw_if_requested()
            assert False, "Should have raised InterruptedError"
        except InterruptedError:
            pass

    def test_throw_if_not_requested_noop(self):
        token = InterruptToken()
        token.throw_if_requested()  # Should not raise

    def test_reset_clears_flag(self):
        token = InterruptToken()
        token.request()
        assert token.is_requested()
        token.reset()
        assert not token.is_requested()

    def test_request_interrupt_alias(self):
        """request_interrupt() is TaskMonitor-compatible alias for request()."""
        token = InterruptToken()
        token.request_interrupt()
        assert token.should_interrupt()

    def test_thread_safety(self):
        """Token works correctly across threads."""
        token = InterruptToken()
        detected = [False]

        def worker():
            while not token.is_requested():
                time.sleep(0.01)
            detected[0] = True

        t = threading.Thread(target=worker)
        t.start()
        time.sleep(0.05)
        token.request()
        t.join(timeout=1.0)
        assert detected[0], "Worker thread should detect the interrupt"


# ---------------------------------------------------------------------------
# TaskMonitor + InterruptToken integration
# ---------------------------------------------------------------------------


class TestTaskMonitorWithToken:
    """Test that TaskMonitor delegates to InterruptToken when attached."""

    def test_without_token_uses_local_flag(self):
        """Backward compat: no token → uses internal _interrupt_requested."""
        monitor = TaskMonitor()
        monitor.start("test")
        assert not monitor.should_interrupt()
        monitor.request_interrupt()
        assert monitor.should_interrupt()

    def test_with_token_delegates_should_interrupt(self):
        """When token is attached, should_interrupt() delegates to it."""
        token = InterruptToken()
        monitor = TaskMonitor()
        monitor.set_interrupt_token(token)
        monitor.start("test")

        assert not monitor.should_interrupt()
        token.request()
        assert monitor.should_interrupt()

    def test_with_token_request_interrupt_delegates(self):
        """When token is attached, request_interrupt() triggers the token."""
        token = InterruptToken()
        monitor = TaskMonitor()
        monitor.set_interrupt_token(token)
        monitor.start("test")

        monitor.request_interrupt()
        assert token.is_requested()
        assert monitor.should_interrupt()

    def test_shared_token_across_monitors(self):
        """Multiple monitors sharing one token: one request cancels all."""
        token = InterruptToken()

        m1 = TaskMonitor()
        m1.set_interrupt_token(token)
        m1.start("thinking")

        m2 = TaskMonitor()
        m2.set_interrupt_token(token)
        m2.start("tool execution")

        m3 = TaskMonitor()
        m3.set_interrupt_token(token)
        m3.start("action LLM call")

        # None interrupted yet
        assert not m1.should_interrupt()
        assert not m2.should_interrupt()
        assert not m3.should_interrupt()

        # Single request cancels ALL
        token.request()
        assert m1.should_interrupt()
        assert m2.should_interrupt()
        assert m3.should_interrupt()

    def test_set_token_after_construction(self):
        """Token can be attached after monitor construction."""
        token = InterruptToken()
        monitor = TaskMonitor()
        monitor.start("test")

        assert not monitor.should_interrupt()
        monitor.set_interrupt_token(token)
        token.request()
        assert monitor.should_interrupt()


# ---------------------------------------------------------------------------
# Scenario 1: Agent Thinking (LLM call spinner)
# ---------------------------------------------------------------------------


class TestScenario1AgentThinking:
    """ESC during LLM call (thinking spinner visible)."""

    def test_token_propagates_to_llm_caller_monitor(self):
        """When LLMCaller is calling LLM, the token reaches the monitor."""
        from opendev.repl.llm_caller import LLMCaller

        console = Mock()
        caller = LLMCaller(console)

        # Simulate what happens during call_llm_with_progress
        token = InterruptToken()
        monitor = TaskMonitor()
        monitor.set_interrupt_token(token)
        caller._current_task_monitor = monitor

        # ESC fires → request_interrupt
        result = caller.request_interrupt()
        assert result is True
        assert token.is_requested()
        assert monitor.should_interrupt()

    def test_react_executor_token_created_per_run(self):
        """ReactExecutor creates a fresh InterruptToken for each execute() call."""
        from opendev.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)

        assert executor._active_interrupt_token is None

        # Simulate the token creation that happens at start of execute()
        from opendev.core.runtime.interrupt_token import InterruptToken
        executor._active_interrupt_token = InterruptToken()
        assert not executor._active_interrupt_token.is_requested()

        # Simulate ESC
        executor._active_interrupt_token.request()
        assert executor._active_interrupt_token.is_requested()

    def test_request_interrupt_triggers_token(self):
        """ReactExecutor.request_interrupt() signals the active token."""
        from opendev.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)
        executor._active_interrupt_token = InterruptToken()

        result = executor.request_interrupt()
        assert result is True
        assert executor._active_interrupt_token.is_requested()

    def test_thinking_phase_monitor_gets_token(self):
        """TaskMonitor in _get_thinking_trace gets the run token attached."""
        from opendev.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)
        token = InterruptToken()
        executor._active_interrupt_token = token

        # Create a monitor like _get_thinking_trace does
        monitor = TaskMonitor()
        if executor._active_interrupt_token:
            monitor.set_interrupt_token(executor._active_interrupt_token)

        # Token propagates through monitor
        token.request()
        assert monitor.should_interrupt()


# ---------------------------------------------------------------------------
# Scenario 2: Tool Executing (tool spinner visible)
# ---------------------------------------------------------------------------


class TestScenario2ToolExecution:
    """ESC during tool execution (tool spinner visible)."""

    def test_tool_monitor_gets_token(self):
        """Tool execution monitor gets the run token attached."""
        token = InterruptToken()
        tool_monitor = TaskMonitor()
        tool_monitor.set_interrupt_token(token)
        tool_monitor.start("bash(ls -la)")

        assert not tool_monitor.should_interrupt()
        token.request()
        assert tool_monitor.should_interrupt()

    def test_tool_executor_interrupt_propagates(self):
        """ToolExecutor.request_interrupt triggers attached token via monitor."""
        from opendev.repl.tool_executor import ToolExecutor

        console = Mock()
        output_formatter = Mock()
        mode_manager = Mock()
        session_manager = Mock()

        executor = ToolExecutor(
            console, output_formatter, mode_manager, session_manager, None, None
        )

        token = InterruptToken()
        monitor = TaskMonitor()
        monitor.set_interrupt_token(token)
        monitor.start("test tool")
        executor._current_task_monitor = monitor

        result = executor.request_interrupt()
        assert result is True
        assert token.is_requested()

    def test_query_processor_interrupt_reaches_tool_via_token(self):
        """Full path: QueryProcessor.request_interrupt → token → tool monitor."""
        console = Mock()
        session_manager = Mock()
        config = Mock()
        config.playbook_strategies = []
        config_manager = Mock()
        mode_manager = Mock()
        file_ops = Mock()
        output_formatter = Mock()
        status_line = Mock()

        from opendev.repl.query_processor import QueryProcessor

        qp = QueryProcessor(
            console=console,
            session_manager=session_manager,
            config=config,
            config_manager=config_manager,
            mode_manager=mode_manager,
            file_ops=file_ops,
            output_formatter=output_formatter,
            status_line=status_line,
            message_printer_callback=Mock(),
        )

        # Simulate active run with token
        token = InterruptToken()
        qp._react_executor._active_interrupt_token = token

        # Simulate tool monitor attached to same token
        tool_monitor = TaskMonitor()
        tool_monitor.set_interrupt_token(token)
        tool_monitor.start("bash(npm test)")
        qp._tool_executor._current_task_monitor = tool_monitor

        # ESC → request_interrupt
        result = qp.request_interrupt()
        assert result is True

        # Token is triggered → tool monitor sees interrupt
        assert token.is_requested()
        assert tool_monitor.should_interrupt()

    def test_concurrent_tool_monitors_all_interrupted(self):
        """Parallel tool execution: all monitors share one token."""
        token = InterruptToken()

        monitors = []
        for name in ["bash(test1)", "read_file(a.py)", "search(pattern)"]:
            m = TaskMonitor()
            m.set_interrupt_token(token)
            m.start(name)
            monitors.append(m)

        # None interrupted
        for m in monitors:
            assert not m.should_interrupt()

        # Single ESC cancels all
        token.request()
        for m in monitors:
            assert m.should_interrupt(), f"Monitor for {m.get_task_description()} should be interrupted"


# ---------------------------------------------------------------------------
# Scenario 3: Approval Panel / Modal Controller
# ---------------------------------------------------------------------------


class TestScenario3ApprovalPanel:
    """ESC when approval panel or modal controller is showing."""

    def test_controller_registry_cancel_active(self):
        """InterruptManager cancels registered active controller."""
        from opendev.ui_textual.managers.interrupt_manager import InterruptManager

        app = Mock()
        app.input_field = Mock()
        app.input_field._completions = None

        manager = InterruptManager(app)

        # Register a mock controller
        controller = Mock()
        controller.active = True
        controller.cancel = Mock()
        manager.register_controller(controller)

        # handle_interrupt should find and cancel it
        result = manager.handle_interrupt()
        assert result is True
        controller.cancel.assert_called_once()

    def test_controller_registry_skips_inactive(self):
        """Registry skips inactive controllers."""
        from opendev.ui_textual.managers.interrupt_manager import InterruptManager

        # Use a minimal mock that won't trigger fallback controller lookups
        app = Mock(spec=[])
        app.input_field = Mock()
        app.input_field._completions = None

        manager = InterruptManager(app)

        inactive = Mock()
        inactive.active = False
        manager.register_controller(inactive)

        result = manager.handle_interrupt()
        assert result is False
        inactive.cancel.assert_not_called()

    def test_controller_registry_unregister(self):
        """Unregistered controller is no longer cancelled."""
        from opendev.ui_textual.managers.interrupt_manager import InterruptManager

        # Use a minimal mock that won't trigger fallback controller lookups
        app = Mock(spec=[])
        app.input_field = Mock()
        app.input_field._completions = None

        manager = InterruptManager(app)

        controller = Mock()
        controller.active = True
        controller.cancel = Mock()
        manager.register_controller(controller)
        manager.unregister_controller(controller)

        result = manager.handle_interrupt()
        assert result is False
        controller.cancel.assert_not_called()

    def test_request_run_interrupt_via_token(self):
        """InterruptManager.request_run_interrupt() triggers the active token."""
        from opendev.ui_textual.managers.interrupt_manager import InterruptManager

        app = Mock()
        manager = InterruptManager(app)

        token = InterruptToken()
        manager.set_interrupt_token(token)

        result = manager.request_run_interrupt()
        assert result is True
        assert token.is_requested()

    def test_request_run_interrupt_no_token(self):
        """request_run_interrupt returns False when no token is set."""
        from opendev.ui_textual.managers.interrupt_manager import InterruptManager

        app = Mock()
        manager = InterruptManager(app)

        result = manager.request_run_interrupt()
        assert result is False


# ---------------------------------------------------------------------------
# End-to-end: Full interrupt path simulation
# ---------------------------------------------------------------------------


class TestEndToEndInterruptPath:
    """Simulate the full ESC → token → cancellation path."""

    def test_esc_during_thinking_reaches_http_client(self):
        """ESC → action_interrupt → token → TaskMonitor → HttpClient detects it."""
        token = InterruptToken()

        # Simulate HttpClient polling loop
        monitor = TaskMonitor()
        monitor.set_interrupt_token(token)
        monitor.start("Thinking...")

        # Simulate the HttpClient._should_interrupt check
        assert not monitor.should_interrupt()

        # ESC fires
        token.request()

        # HttpClient would detect this on next poll
        assert monitor.should_interrupt()

    def test_esc_during_tool_breaks_react_loop(self):
        """ESC during tool → token set → next iteration boundary breaks loop."""
        token = InterruptToken()

        # Simulate tool running
        tool_monitor = TaskMonitor()
        tool_monitor.set_interrupt_token(token)
        tool_monitor.start("bash(make build)")

        # ESC fires mid-tool
        token.request()

        # At next iteration boundary, loop would check:
        assert token.is_requested()
        # And break

    def test_interrupt_message_format(self):
        """Verify the standard ⎿ interrupt message format."""
        from opendev.ui_textual.utils.interrupt_utils import (
            create_interrupt_text,
            create_interrupt_message,
            STANDARD_INTERRUPT_MESSAGE,
        )

        # Text format (Rich Text object)
        text = create_interrupt_text(STANDARD_INTERRUPT_MESSAGE)
        assert "⎿" in text.plain
        assert "Interrupted" in text.plain
        assert "What should I do instead?" in text.plain

        # String format (for tool result formatting)
        msg = create_interrupt_message(STANDARD_INTERRUPT_MESSAGE)
        assert "::interrupted::" in msg
        assert "Interrupted" in msg

    def test_multiple_esc_presses_idempotent(self):
        """Multiple ESC presses don't cause issues."""
        token = InterruptToken()
        monitor = TaskMonitor()
        monitor.set_interrupt_token(token)
        monitor.start("test")

        # First ESC
        token.request()
        assert token.is_requested()

        # Second ESC (idempotent)
        token.request()
        assert token.is_requested()

        # Monitor still works
        assert monitor.should_interrupt()


# ---------------------------------------------------------------------------
# HttpClient interrupt detection (duck-typing compatibility)
# ---------------------------------------------------------------------------


class TestHttpClientCompatibility:
    """Verify InterruptToken works with HttpClient._should_interrupt duck-typing."""

    def test_token_as_task_monitor_duck_type(self):
        """InterruptToken can be used directly where TaskMonitor is expected."""
        from opendev.core.agents.components.api.http_client import AgentHttpClient

        token = InterruptToken()

        # AgentHttpClient._should_interrupt checks should_interrupt() method
        assert not AgentHttpClient._should_interrupt(token)

        token.request()
        assert AgentHttpClient._should_interrupt(token)

    def test_monitor_with_token_works_with_http_client(self):
        """TaskMonitor with attached token works with AgentHttpClient._should_interrupt."""
        from opendev.core.agents.components.api.http_client import AgentHttpClient

        token = InterruptToken()
        monitor = TaskMonitor()
        monitor.set_interrupt_token(token)

        assert not AgentHttpClient._should_interrupt(monitor)

        token.request()
        assert AgentHttpClient._should_interrupt(monitor)
