"""Tests for interrupt guards.

Fix 3: on_interrupt guard prevents duplicates
Fix 4: on_interrupt called in finally block of execute()
Fix B: on_thinking() spinner guard
"""

from unittest.mock import Mock

from opendev.core.runtime.interrupt_token import InterruptToken


# ---------------------------------------------------------------------------
# Fix 3: on_interrupt guard prevents duplicates
# ---------------------------------------------------------------------------


class TestFix3InterruptGuard:
    """Verify _interrupt_shown prevents duplicate messages."""

    def _make_callback(self):
        """Create a TextualUICallback with mocked dependencies."""
        from opendev.ui_textual.ui_callback import TextualUICallback

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
        from opendev.repl.react_executor import ReactExecutor

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

        executor = ReactExecutor(session_manager, config, mode_manager=Mock(), console=console, llm_caller=llm_caller, tool_executor=tool_executor)
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
# Fix B: on_thinking() spinner guard
# ---------------------------------------------------------------------------


class TestFixBThinkingSpinnerGuard:
    """Verify on_thinking() doesn't restart spinner when interrupted."""

    def _make_callback(self, token=None):
        """Create a TextualUICallback with optional interrupt token."""
        from opendev.ui_textual.ui_callback import TextualUICallback

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
