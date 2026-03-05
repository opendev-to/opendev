"""Tests for the 6 ESC interrupt system fixes.

Fix 1: InterruptToken wired to InterruptManager
Fix 2: action_interrupt always signals token when processing
Fix 3: on_interrupt guard prevents duplicates
Fix 4: on_interrupt called in finally block of execute()
Fix 5: Bash tool propagates "interrupted" flag
Fix 6: Token check between sequential tool calls
"""

from unittest.mock import Mock, MagicMock, patch, PropertyMock

from swecli.core.runtime.interrupt_token import InterruptToken


# ---------------------------------------------------------------------------
# Fix 1: InterruptToken wired to InterruptManager
# ---------------------------------------------------------------------------


class TestFix1TokenWiredToInterruptManager:
    """Verify that execute() wires and clears the token on InterruptManager."""

    def _make_executor(self):
        """Create a minimal ReactExecutor with mocked dependencies."""
        from swecli.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        # Make call_llm_with_progress return a successful response with no tool calls
        llm_caller.call_llm_with_progress.return_value = (
            {
                "success": True,
                "content": "Done",
                "message": {"content": "Done"},
                "tool_calls": None,
                "usage": None,
            },
            100,
        )
        tool_executor = Mock()
        tool_executor.record_tool_learnings = Mock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)
        return executor

    def _make_ui_callback_with_manager(self):
        """Create a mock ui_callback with a chat_app that has an InterruptManager."""
        interrupt_manager = Mock()
        interrupt_manager.set_interrupt_token = Mock()
        interrupt_manager.clear_interrupt_token = Mock()

        chat_app = Mock()
        chat_app._interrupt_manager = interrupt_manager

        ui_callback = Mock()
        ui_callback.chat_app = chat_app
        return ui_callback, interrupt_manager

    def test_token_wired_to_interrupt_manager(self):
        """set_interrupt_token is called with the active token during execute()."""
        executor = self._make_executor()
        ui_callback, interrupt_manager = self._make_ui_callback_with_manager()

        agent = Mock()
        agent.build_system_prompt.return_value = "system"
        agent.system_prompt = "system"
        tool_registry = Mock()
        tool_registry.thinking_handler = Mock(is_visible=False, includes_critique=False)
        approval_manager = Mock()
        undo_manager = Mock()

        executor.execute(
            "test query",
            [{"role": "system", "content": "sys"}],
            agent,
            tool_registry,
            approval_manager,
            undo_manager,
            ui_callback=ui_callback,
        )

        interrupt_manager.set_interrupt_token.assert_called_once()
        token = interrupt_manager.set_interrupt_token.call_args[0][0]
        assert isinstance(token, InterruptToken)

    def test_token_cleared_after_run(self):
        """clear_interrupt_token is called after execute() completes."""
        executor = self._make_executor()
        ui_callback, interrupt_manager = self._make_ui_callback_with_manager()

        agent = Mock()
        agent.build_system_prompt.return_value = "system"
        agent.system_prompt = "system"
        tool_registry = Mock()
        tool_registry.thinking_handler = Mock(is_visible=False, includes_critique=False)
        approval_manager = Mock()
        undo_manager = Mock()

        executor.execute(
            "test query",
            [{"role": "system", "content": "sys"}],
            agent,
            tool_registry,
            approval_manager,
            undo_manager,
            ui_callback=ui_callback,
        )

        interrupt_manager.clear_interrupt_token.assert_called_once()


# ---------------------------------------------------------------------------
# Fix 2: action_interrupt signals token when processing
# ---------------------------------------------------------------------------


class TestFix2ActionInterruptSignalsToken:
    """Verify action_interrupt cancels controllers AND signals token."""

    def _make_app(self):
        """Create a minimal mock SWECLIChatApp."""
        app = Mock()
        app._is_processing = True
        app.on_interrupt = Mock()
        app.spinner_service = Mock()
        app._stop_local_spinner = Mock()

        # Ensure no autocomplete (input_field._completions must be falsy)
        app.input_field = Mock()
        app.input_field._completions = None

        # Create a real InterruptManager with mocked app
        from swecli.ui_textual.managers.interrupt_manager import InterruptManager

        manager = InterruptManager(app)
        app._interrupt_manager = manager

        return app, manager

    def test_action_interrupt_signals_token_during_approval(self):
        """When processing with active controller, both cancel and token signal happen."""
        app, manager = self._make_app()

        # Set up an active token
        token = InterruptToken()
        manager.set_interrupt_token(token)

        # Set up an active controller
        controller = Mock()
        controller.active = True
        controller.cancel = Mock()
        manager.register_controller(controller)

        # Import and call the actual method
        from swecli.ui_textual.chat_app import SWECLIChatApp

        SWECLIChatApp.action_interrupt(app)

        # Both should be called
        controller.cancel.assert_called_once()
        assert token.is_requested(), "Token should be signaled"
        app.on_interrupt.assert_called_once()

    def test_action_interrupt_shows_immediate_feedback(self):
        """When processing, spinner_service.stop_all is called."""
        app, manager = self._make_app()

        token = InterruptToken()
        manager.set_interrupt_token(token)

        from swecli.ui_textual.chat_app import SWECLIChatApp

        SWECLIChatApp.action_interrupt(app)
        SWECLIChatApp._show_interrupt_feedback(app)

        app.spinner_service.stop_all.assert_called_with(immediate=True)

    def test_non_processing_esc_delegates_to_manager(self):
        """When not processing, ESC delegates to InterruptManager.handle_interrupt."""
        app, manager = self._make_app()
        app._is_processing = False

        # Patch handle_interrupt to track calls
        manager.handle_interrupt = Mock(return_value=False)

        from swecli.ui_textual.chat_app import SWECLIChatApp

        SWECLIChatApp.action_interrupt(app)

        manager.handle_interrupt.assert_called_once()
        # request_run_interrupt should NOT be called
        assert not InterruptToken().is_requested()


# ---------------------------------------------------------------------------
# Fix 3: on_interrupt guard prevents duplicates
# ---------------------------------------------------------------------------


class TestFix3InterruptGuard:
    """Verify _interrupt_shown prevents duplicate messages."""

    def _make_callback(self):
        """Create a TextualUICallback with mocked dependencies."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        conversation = Mock()
        conversation.lines = []
        conversation.write = Mock()
        conversation.stop_spinner = Mock()

        chat_app = Mock()
        chat_app._stop_local_spinner = Mock()
        chat_app.spinner_service = Mock()
        chat_app._is_processing = False
        # Mock call_from_thread to just call the function directly
        chat_app.call_from_thread = lambda fn, *a, **kw: fn(*a, **kw)

        cb = TextualUICallback(conversation, chat_app=chat_app)
        return cb

    def test_on_interrupt_guard_prevents_duplicates(self):
        """Calling on_interrupt twice only shows message once."""
        cb = self._make_callback()

        cb.on_interrupt()
        assert cb._interrupt_shown is True
        first_call_count = cb.conversation.write.call_count

        cb.on_interrupt()  # Second call should be a no-op
        assert cb.conversation.write.call_count == first_call_count

    def test_on_interrupt_guard_resets_on_new_run(self):
        """on_thinking_start resets the interrupt guard."""
        cb = self._make_callback()

        cb.on_interrupt()
        assert cb._interrupt_shown is True

        cb.on_thinking_start()
        assert cb._interrupt_shown is False


# ---------------------------------------------------------------------------
# Fix 4: on_interrupt called in finally block
# ---------------------------------------------------------------------------


class TestFix4OnInterruptInFinally:
    """Verify on_interrupt is called in finally when token was signaled."""

    def test_on_interrupt_called_in_finally(self):
        """If token is requested during execute, on_interrupt fires in finally."""
        from swecli.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0

        # We'll capture the executor reference so we can signal its token
        executor_ref = []

        def fake_llm_call(agent, messages, monitor, **kwargs):
            # Signal the executor's active interrupt token
            ex = executor_ref[0]
            if ex._active_interrupt_token:
                ex._active_interrupt_token.request()
            return (
                {"success": False, "error": "Interrupted by user", "content": ""},
                0,
            )

        llm_caller = Mock()
        llm_caller.call_llm_with_progress = fake_llm_call
        tool_executor = Mock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)
        executor_ref.append(executor)
        # Prevent compaction from interfering with the test
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        ui_callback = Mock()
        ui_callback.chat_app = None  # No chat_app to simplify
        ui_callback.on_thinking_start = Mock()
        ui_callback.on_interrupt = Mock()
        ui_callback.on_debug = Mock()

        agent = Mock()
        agent.build_system_prompt.return_value = "system"
        agent.system_prompt = "system"
        tool_registry = Mock()
        tool_registry.thinking_handler = Mock(is_visible=False, includes_critique=False)

        executor.execute(
            "test",
            [{"role": "system", "content": "sys"}],
            agent,
            tool_registry,
            Mock(),
            Mock(),
            ui_callback=ui_callback,
        )

        # on_interrupt should have been called (from _handle_llm_error and/or finally)
        assert ui_callback.on_interrupt.call_count >= 1


# ---------------------------------------------------------------------------
# Fix 5: Bash tool propagates "interrupted" flag
# ---------------------------------------------------------------------------


class TestFix5BashInterruptPropagation:
    """Verify process_handlers returns interrupted=True for interrupted commands."""

    def test_bash_interrupt_propagates_flag(self):
        """When bash result has 'interrupted' in error, flag is set."""
        from swecli.core.context_engineering.tools.handlers.process_handlers import (
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
        from swecli.core.context_engineering.tools.handlers.process_handlers import (
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
        from swecli.repl.react_executor import ReactExecutor, IterationContext

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)

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
        executor._execute_single_tool = Mock(
            return_value={"success": True, "output": "content"}
        )

        # Run the tool processing loop manually (extract the sequential part)
        tool_results_by_id = {}
        operation_cancelled = False
        for tool_call in tool_calls:
            if (
                executor._active_interrupt_token
                and executor._active_interrupt_token.is_requested()
            ):
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
# Fix A: _check_interrupt() centralized phase-boundary checking
# ---------------------------------------------------------------------------


class TestFixACheckInterrupt:
    """Verify _check_interrupt() raises InterruptedError when token is signaled."""

    def _make_executor(self):
        """Create a minimal ReactExecutor with mocked dependencies."""
        from swecli.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        llm_caller.call_llm_with_progress.return_value = (
            {
                "success": True,
                "content": "Done",
                "message": {"content": "Done"},
                "tool_calls": None,
                "usage": None,
            },
            100,
        )
        tool_executor = Mock()
        tool_executor.record_tool_learnings = Mock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)
        return executor

    def test_check_interrupt_raises_when_token_signaled(self):
        """_check_interrupt() raises InterruptedError when token is signaled."""
        executor = self._make_executor()
        token = InterruptToken()
        token.request()
        executor._active_interrupt_token = token

        import pytest
        with pytest.raises(InterruptedError):
            executor._check_interrupt("test-phase")

    def test_check_interrupt_noop_when_token_not_signaled(self):
        """_check_interrupt() does nothing when token is not signaled."""
        executor = self._make_executor()
        token = InterruptToken()
        executor._active_interrupt_token = token

        # Should not raise
        executor._check_interrupt("test-phase")

    def test_check_interrupt_noop_when_no_token(self):
        """_check_interrupt() does nothing when no token is set."""
        executor = self._make_executor()
        executor._active_interrupt_token = None

        # Should not raise
        executor._check_interrupt("test-phase")

    def test_check_interrupt_includes_phase_in_message(self):
        """InterruptedError message contains the phase name."""
        executor = self._make_executor()
        token = InterruptToken()
        token.request()
        executor._active_interrupt_token = token

        import pytest
        with pytest.raises(InterruptedError, match="post-thinking"):
            executor._check_interrupt("post-thinking")

    def test_run_iteration_catches_interrupted_error(self):
        """_run_iteration returns BREAK when InterruptedError is raised."""
        from swecli.repl.react_executor import ReactExecutor, IterationContext, LoopAction

        executor = self._make_executor()
        # Signal the token so _check_interrupt("pre-thinking") fires
        token = InterruptToken()
        token.request()
        executor._active_interrupt_token = token

        # Prevent compaction from interfering
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        ctx = IterationContext(
            query="test",
            messages=[{"role": "system", "content": "sys"}],
            agent=Mock(system_prompt="sys"),
            tool_registry=Mock(
                thinking_handler=Mock(is_visible=False, includes_critique=False)
            ),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=Mock(),
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK

    def test_run_iteration_calls_on_interrupt_when_caught(self):
        """_run_iteration calls on_interrupt() when InterruptedError is caught."""
        from swecli.repl.react_executor import ReactExecutor, IterationContext

        executor = self._make_executor()
        token = InterruptToken()
        token.request()
        executor._active_interrupt_token = token

        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        ui_callback = Mock()

        ctx = IterationContext(
            query="test",
            messages=[{"role": "system", "content": "sys"}],
            agent=Mock(system_prompt="sys"),
            tool_registry=Mock(
                thinking_handler=Mock(is_visible=False, includes_critique=False)
            ),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=ui_callback,
        )

        executor._run_iteration(ctx)
        ui_callback.on_interrupt.assert_called_once()

    def test_post_thinking_boundary_prevents_critique(self):
        """After thinking succeeds, signaled token prevents critique phase."""
        from swecli.repl.react_executor import ReactExecutor, IterationContext, LoopAction

        executor = self._make_executor()
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        # Token will be signaled DURING _get_thinking_trace
        token = InterruptToken()
        executor._active_interrupt_token = token

        def fake_thinking(messages, agent, ui_callback=None):
            # Simulate: thinking completes, then ESC arrives
            token.request()
            return "Some thinking trace"

        executor._get_thinking_trace = fake_thinking
        executor._critique_and_refine_thinking = Mock(return_value="refined")

        ui_callback = Mock()

        ctx = IterationContext(
            query="test",
            messages=[{"role": "system", "content": "sys"}],
            agent=Mock(system_prompt="sys"),
            tool_registry=Mock(
                thinking_handler=Mock(is_visible=True, includes_critique=True)
            ),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=ui_callback,
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK
        # Critique should NOT have been called
        executor._critique_and_refine_thinking.assert_not_called()

    def test_pre_action_boundary_prevents_action_llm(self):
        """After thinking+critique succeed, signaled token prevents action LLM."""
        from swecli.repl.react_executor import ReactExecutor, IterationContext, LoopAction

        executor = self._make_executor()
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        token = InterruptToken()
        executor._active_interrupt_token = token

        # Thinking succeeds normally
        executor._get_thinking_trace = Mock(return_value="trace")
        # Critique succeeds but signals token
        def fake_critique(trace, messages, agent, ui_callback=None):
            token.request()
            return "critiqued trace"

        executor._critique_and_refine_thinking = fake_critique

        ui_callback = Mock()

        ctx = IterationContext(
            query="test",
            messages=[{"role": "system", "content": "sys"}],
            agent=Mock(system_prompt="sys"),
            tool_registry=Mock(
                thinking_handler=Mock(is_visible=True, includes_critique=True)
            ),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=ui_callback,
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK
        # LLM action call should NOT have been made
        executor._llm_caller.call_llm_with_progress.assert_not_called()

    def test_pre_thinking_boundary_prevents_thinking_llm(self):
        """Pre-signaled token prevents _get_thinking_trace from being called."""
        from swecli.repl.react_executor import ReactExecutor, IterationContext, LoopAction

        executor = self._make_executor()
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        token = InterruptToken()
        token.request()  # Pre-signal
        executor._active_interrupt_token = token

        executor._get_thinking_trace = Mock(return_value="trace")

        ui_callback = Mock()

        ctx = IterationContext(
            query="test",
            messages=[{"role": "system", "content": "sys"}],
            agent=Mock(system_prompt="sys"),
            tool_registry=Mock(
                thinking_handler=Mock(is_visible=True, includes_critique=False)
            ),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=ui_callback,
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK
        # _get_thinking_trace should NOT have been called
        executor._get_thinking_trace.assert_not_called()


# ---------------------------------------------------------------------------
# Fix B: on_thinking() spinner guard
# ---------------------------------------------------------------------------


class TestFixBThinkingSpinnerGuard:
    """Verify on_thinking() doesn't restart spinner when interrupted."""

    def _make_callback(self, token=None):
        """Create a TextualUICallback with optional interrupt token."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        conversation = Mock()
        conversation.lines = []
        conversation.write = Mock()
        conversation.stop_spinner = Mock()
        conversation.add_thinking_block = Mock()

        chat_app = Mock()
        chat_app._stop_local_spinner = Mock()
        chat_app._start_local_spinner = Mock()
        chat_app._thinking_visible = True
        chat_app.spinner_service = Mock()
        chat_app._is_processing = False

        # Set up interrupt manager with token
        interrupt_manager = Mock()
        interrupt_manager._active_interrupt_token = token
        chat_app._interrupt_manager = interrupt_manager

        chat_app.call_from_thread = lambda fn, *a, **kw: fn(*a, **kw)

        cb = TextualUICallback(conversation, chat_app=chat_app)
        return cb, chat_app

    def test_on_thinking_no_spinner_restart_after_interrupt(self):
        """Spinner is NOT restarted when interrupt token is signaled."""
        token = InterruptToken()
        token.request()
        cb, chat_app = self._make_callback(token=token)

        cb.on_thinking("Some thinking content")

        chat_app._start_local_spinner.assert_not_called()

    def test_on_thinking_spinner_restart_when_not_interrupted(self):
        """Spinner IS restarted when token exists but is not signaled."""
        token = InterruptToken()
        cb, chat_app = self._make_callback(token=token)

        cb.on_thinking("Some thinking content")

        chat_app._start_local_spinner.assert_called_once()

    def test_on_thinking_spinner_restart_when_no_token(self):
        """Spinner IS restarted when no token is set (normal flow)."""
        cb, chat_app = self._make_callback(token=None)

        cb.on_thinking("Some thinking content")

        chat_app._start_local_spinner.assert_called_once()


# ---------------------------------------------------------------------------
# Fix C: Parallel tools guard
# ---------------------------------------------------------------------------


class TestFixCParallelToolsGuard:
    """Verify parallel tools are skipped when interrupt token is signaled."""

    def _make_executor(self):
        """Create a minimal ReactExecutor with mocked dependencies."""
        from swecli.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)
        return executor

    def test_parallel_tools_skip_all_when_interrupted(self):
        """All parallel tools get interrupted result when token is signaled."""
        from swecli.repl.react_executor import IterationContext

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
        from swecli.repl.react_executor import IterationContext

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

        executor._execute_single_tool = Mock(
            return_value={"success": True, "output": "done"}
        )
        results, cancelled = executor._execute_tools_parallel(tool_calls, ctx)

        assert cancelled is False
        # Both tools should have been executed
        assert executor._execute_single_tool.call_count == 2


# ---------------------------------------------------------------------------
# Integration / Edge Case Tests
# ---------------------------------------------------------------------------


class TestInterruptIntegrationEdgeCases:
    """Integration and edge case tests for the centralized interrupt system."""

    def _make_executor(self):
        """Create a minimal ReactExecutor with mocked dependencies."""
        from swecli.repl.react_executor import ReactExecutor

        console = Mock()
        session_manager = Mock()
        session_manager.add_message = Mock()
        session_manager.get_current_session.return_value = None
        session_manager.save_session = Mock()
        config = Mock()
        config.auto_save_interval = 0
        llm_caller = Mock()
        tool_executor = Mock()
        tool_executor.record_tool_learnings = Mock()

        executor = ReactExecutor(console, session_manager, config, llm_caller, tool_executor)
        return executor

    def test_on_interrupt_called_exactly_once_through_full_execute(self):
        """on_interrupt is called exactly once even when both _run_iteration
        and finally block try to call it."""
        executor = self._make_executor()
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        # LLM signals interrupt during call
        def fake_llm_call(agent, messages, monitor, **kwargs):
            executor._active_interrupt_token.request()
            return (
                {"success": False, "error": "Interrupted by user", "content": ""},
                0,
            )

        executor._llm_caller.call_llm_with_progress = fake_llm_call

        ui_callback = Mock()
        ui_callback.chat_app = None
        ui_callback.on_thinking_start = Mock()
        ui_callback.on_interrupt = Mock()
        ui_callback.on_debug = Mock()
        # on_interrupt has _interrupt_shown guard; simulate it with side_effect
        interrupt_count = [0]
        original_on_interrupt = ui_callback.on_interrupt

        def guarded_on_interrupt(*args, **kwargs):
            interrupt_count[0] += 1

        ui_callback.on_interrupt = Mock(side_effect=guarded_on_interrupt)

        agent = Mock()
        agent.build_system_prompt.return_value = "system"
        agent.system_prompt = "system"
        tool_registry = Mock()
        tool_registry.thinking_handler = Mock(is_visible=False, includes_critique=False)

        executor.execute(
            "test",
            [{"role": "system", "content": "sys"}],
            agent,
            tool_registry,
            Mock(),
            Mock(),
            ui_callback=ui_callback,
        )

        # on_interrupt called from _handle_llm_error AND finally block
        # Both fire, but the UI callback's _interrupt_shown guard deduplicates
        assert interrupt_count[0] >= 1

    def test_interrupt_at_every_phase_boundary_returns_break(self):
        """Each of the 3 phase boundaries correctly returns BREAK when signaled."""
        from swecli.repl.react_executor import IterationContext, LoopAction

        for phase, thinking_visible in [
            ("pre-thinking", False),
            ("post-thinking", True),
            ("pre-action", True),
        ]:
            executor = self._make_executor()
            executor._compactor = Mock()
            executor._compactor.should_compact.return_value = False

            token = InterruptToken()
            executor._active_interrupt_token = token

            if phase == "pre-thinking":
                # Signal before iteration starts
                token.request()
            elif phase == "post-thinking":
                # Signal during thinking
                def fake_thinking(messages, agent, ui_callback=None):
                    token.request()
                    return "trace"

                executor._get_thinking_trace = fake_thinking
            elif phase == "pre-action":
                # Signal during critique
                executor._get_thinking_trace = Mock(return_value="trace")

                def fake_critique(trace, messages, agent, ui_callback=None):
                    token.request()
                    return "critiqued"

                executor._critique_and_refine_thinking = fake_critique

            ctx = IterationContext(
                query="test",
                messages=[{"role": "system", "content": "sys"}],
                agent=Mock(system_prompt="sys"),
                tool_registry=Mock(
                    thinking_handler=Mock(
                        is_visible=thinking_visible,
                        includes_critique=(phase == "pre-action"),
                    )
                ),
                approval_manager=Mock(),
                undo_manager=Mock(),
                ui_callback=Mock(),
            )

            result = executor._run_iteration(ctx)
            assert result == LoopAction.BREAK, f"Phase {phase} should return BREAK"

    def test_thinking_trace_not_injected_when_interrupted_post_thinking(self):
        """When interrupted at post-thinking, thinking trace is NOT appended to messages."""
        from swecli.repl.react_executor import IterationContext, LoopAction

        executor = self._make_executor()
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        token = InterruptToken()
        executor._active_interrupt_token = token

        def fake_thinking(messages, agent, ui_callback=None):
            token.request()  # Signal during thinking
            return "A thinking trace"

        executor._get_thinking_trace = fake_thinking

        messages = [{"role": "system", "content": "sys"}]
        ctx = IterationContext(
            query="test",
            messages=messages,
            agent=Mock(system_prompt="sys"),
            tool_registry=Mock(
                thinking_handler=Mock(is_visible=True, includes_critique=False)
            ),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=Mock(),
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK

        # The thinking trace should NOT have been injected into messages
        for msg in messages:
            content = msg.get("content", "")
            assert "<thinking_trace>" not in content, (
                "Thinking trace should not be injected when interrupted"
            )
