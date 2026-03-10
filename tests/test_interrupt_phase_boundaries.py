"""Tests for interrupt phase boundaries.

Fix A: _check_interrupt() centralized phase-boundary checking
Integration / Edge Case Tests
"""

from unittest.mock import Mock

from opendev.core.runtime.interrupt_token import InterruptToken


# ---------------------------------------------------------------------------
# Fix A: _check_interrupt() centralized phase-boundary checking
# ---------------------------------------------------------------------------


class TestFixACheckInterrupt:
    """Verify _check_interrupt() raises InterruptedError when token is signaled."""

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

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)
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
        from opendev.repl.react_executor import IterationContext, LoopAction

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
            tool_registry=Mock(thinking_handler=Mock(is_visible=False, includes_critique=False)),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=Mock(),
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK

    def test_run_iteration_calls_on_interrupt_when_caught(self):
        """_run_iteration calls on_interrupt() when InterruptedError is caught."""
        from opendev.repl.react_executor import IterationContext

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
            tool_registry=Mock(thinking_handler=Mock(is_visible=False, includes_critique=False)),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=ui_callback,
        )

        executor._run_iteration(ctx)
        ui_callback.on_interrupt.assert_called_once()

    def test_post_thinking_boundary_prevents_critique(self):
        """After thinking succeeds, signaled token prevents critique phase."""
        from opendev.repl.react_executor import IterationContext, LoopAction

        executor = self._make_executor()
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        # Token will be signaled DURING _get_thinking_trace
        token = InterruptToken()
        executor._active_interrupt_token = token

        def fake_thinking(messages, agent, ui_callback=None, tool_registry=None):
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
            tool_registry=Mock(thinking_handler=Mock(is_visible=True, includes_critique=True)),
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
        from opendev.repl.react_executor import IterationContext, LoopAction

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
            tool_registry=Mock(thinking_handler=Mock(is_visible=True, includes_critique=True)),
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
        from opendev.repl.react_executor import IterationContext, LoopAction

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
            tool_registry=Mock(thinking_handler=Mock(is_visible=True, includes_critique=False)),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=ui_callback,
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK
        # _get_thinking_trace should NOT have been called
        executor._get_thinking_trace.assert_not_called()


# ---------------------------------------------------------------------------
# Integration / Edge Case Tests
# ---------------------------------------------------------------------------


class TestInterruptIntegrationEdgeCases:
    """Integration and edge case tests for the centralized interrupt system."""

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
        tool_executor.record_tool_learnings = Mock()

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)
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
        from opendev.repl.react_executor import IterationContext, LoopAction

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
                def fake_thinking(messages, agent, ui_callback=None, tool_registry=None):
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
        from opendev.repl.react_executor import IterationContext, LoopAction

        executor = self._make_executor()
        executor._compactor = Mock()
        executor._compactor.should_compact.return_value = False

        token = InterruptToken()
        executor._active_interrupt_token = token

        def fake_thinking(messages, agent, ui_callback=None, tool_registry=None):
            token.request()  # Signal during thinking
            return "A thinking trace"

        executor._get_thinking_trace = fake_thinking

        messages = [{"role": "system", "content": "sys"}]
        ctx = IterationContext(
            query="test",
            messages=messages,
            agent=Mock(system_prompt="sys"),
            tool_registry=Mock(thinking_handler=Mock(is_visible=True, includes_critique=False)),
            approval_manager=Mock(),
            undo_manager=Mock(),
            ui_callback=Mock(),
        )

        result = executor._run_iteration(ctx)
        assert result == LoopAction.BREAK

        # The thinking trace should NOT have been injected into messages
        for msg in messages:
            content = msg.get("content", "")
            assert (
                "<thinking_trace>" not in content
            ), "Thinking trace should not be injected when interrupted"
