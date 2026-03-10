"""Tests for interrupt token wiring.

Fix 1: InterruptToken wired to InterruptManager
Fix 2: action_interrupt always signals token when processing
"""

from unittest.mock import Mock

from opendev.core.runtime.interrupt_token import InterruptToken


# ---------------------------------------------------------------------------
# Fix 1: InterruptToken wired to InterruptManager
# ---------------------------------------------------------------------------


class TestFix1TokenWiredToInterruptManager:
    """Verify that execute() wires and clears the token on InterruptManager."""

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

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)
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
        from opendev.ui_textual.managers.interrupt_manager import InterruptManager

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
        from opendev.ui_textual.chat_app import SWECLIChatApp

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

        from opendev.ui_textual.chat_app import SWECLIChatApp

        SWECLIChatApp.action_interrupt(app)
        SWECLIChatApp._show_interrupt_feedback(app)

        app.spinner_service.stop_all.assert_called_with(immediate=True)

    def test_non_processing_esc_delegates_to_manager(self):
        """When not processing, ESC delegates to InterruptManager.handle_interrupt."""
        app, manager = self._make_app()
        app._is_processing = False

        # Patch handle_interrupt to track calls
        manager.handle_interrupt = Mock(return_value=False)

        from opendev.ui_textual.chat_app import SWECLIChatApp

        SWECLIChatApp.action_interrupt(app)

        manager.handle_interrupt.assert_called_once()
        # request_run_interrupt should NOT be called
        assert not InterruptToken().is_requested()
