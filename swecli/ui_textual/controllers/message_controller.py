"""Message submission and processing helpers for the Textual chat app."""

from __future__ import annotations

import logging
from typing import Callable, TYPE_CHECKING

if TYPE_CHECKING:  # pragma: no cover
    from swecli.ui_textual.chat_app import SWECLIChatApp

logger = logging.getLogger(__name__)


class MessageController:
    """Coordinate chat message submission and backend processing."""

    def __init__(self, app: "SWECLIChatApp") -> None:
        self.app = app

    async def submit(self, raw_text: str) -> None:
        """Normalize user input, update history, and trigger processing."""

        app = self.app
        input_field = app.input_field

        if not raw_text.strip():
            input_field.load_text("")
            return

        message_with_placeholders = raw_text.rstrip("\n")
        message = input_field.resolve_large_pastes(message_with_placeholders)

        # Check if already processing BEFORE displaying
        # If processing, message will be queued and displayed when it starts processing
        already_processing = app._is_processing

        self._reset_interaction_state()

        input_field.load_text("")
        input_field.clear_large_pastes()

        if hasattr(app, "_history"):
            app._history.record(message)

        # Only display user message if NOT already processing
        # Queued messages are displayed when they start processing in runner
        # IMPORTANT: Set _is_processing BEFORE display to prevent race condition
        # where rapid submissions see False before the flag is set in _process_message
        if not already_processing:
            app._is_processing = True  # Set immediately to prevent race
            from swecli.ui_textual.debug_logger import debug_log
            debug_log("MessageController", "SET _is_processing=True")
            ledger = getattr(app, "_display_ledger", None)
            if ledger:
                ledger.display_user_message(message, "message_controller")
            else:
                logger.warning(
                    "DisplayLedger not available, falling back to direct "
                    "display (source=%s)", "message_controller"
                )
                app.conversation.add_user_message(message)

        if app._model_picker.active:
            handled = await app._model_picker.handle_input(message.strip())
            if handled:
                return

        stripped_message = message.strip()

        if stripped_message.startswith("/"):
            handled = await app.handle_command(stripped_message)
            if not handled and app.on_message:
                app.on_message(message)
            else:
                # Reset processing state for handled slash commands
                app._is_processing = False
            return

        await self._process_message(message, needs_display=already_processing)

    async def process(self, message: str) -> None:
        """Process a message that has already been recorded in history."""
        await self._process_message(message, needs_display=False)

    async def _process_message(self, message: str, needs_display: bool = False) -> None:
        """Submit message to backend for processing.

        Args:
            message: The message text to process
            needs_display: If True, the message will be displayed in conversation
                          when it starts processing (for queued messages)
        """
        app = self.app
        if not app.on_message:
            app.conversation.add_error("No backend handler configured; unable to process message.")
            return

        self._set_processing_state(True)

        try:
            # Use runner's enqueue_message with needs_display flag if available
            runner = getattr(app, '_runner', None)
            if runner and hasattr(runner, 'enqueue_message'):
                runner.enqueue_message(message, needs_display=needs_display)
            else:
                # Fallback to on_message callback
                app.on_message(message)
        except Exception as exc:  # pragma: no cover - defensive
            self.notify_processing_error(f"Failed to submit message: {exc}")

    def notify_processing_complete(self) -> None:
        """Reset processing flags after the backend finishes."""

        def finalize() -> None:
            self._set_processing_state(False)
            app = self.app
            # Close the display ledger turn so old hashes get cleaned up
            ledger = getattr(app, "_display_ledger", None)
            if ledger:
                ledger.complete_turn("notify_processing_complete")
            if hasattr(app, "_emit_tool_follow_up_if_needed"):
                app._emit_tool_follow_up_if_needed()

        self._invoke_on_ui_thread(finalize)

    def notify_processing_error(self, error: str) -> None:
        """Display an error message and reset state."""

        def finalize() -> None:
            app = self.app
            app.conversation.add_error(error)
            self._set_processing_state(False)
            self._reset_interaction_state()

        self._invoke_on_ui_thread(finalize)

    def _reset_interaction_state(self) -> None:
        app = self.app
        tool_summary = getattr(app, "_tool_summary", None)
        if tool_summary is not None:
            tool_summary.reset()
        console_buffer = getattr(app, "_console_buffer", None)
        if console_buffer is not None:
            console_buffer.clear_assistant_history()

        input_field = getattr(app, "input_field", None)
        if input_field is not None and hasattr(input_field, "_clear_completions"):
            input_field._clear_completions()

    def _set_processing_state(self, active: bool) -> None:
        from swecli.ui_textual.debug_logger import debug_log
        app = self.app
        debug_log("MessageController", f"_set_processing_state called with active={active}, current={app._is_processing}")
        if active == app._is_processing:
            # Even if already in the target state, ensure spinner reflects it
            if active:
                app._start_local_spinner()
            return

        debug_log("MessageController", f"_is_processing changing from {app._is_processing} to {active}")
        app._is_processing = active

        if not hasattr(app, "status_bar"):
            return

        if active:
            app._start_local_spinner()
        else:
            app._stop_local_spinner()

    def set_processing_state(self, active: bool) -> None:
        """Expose processing state toggles for the chat app."""
        self._set_processing_state(active)

    def _invoke_on_ui_thread(self, callback: Callable[[], None]) -> None:
        """Run callback on the UI thread, blocking until complete."""
        self.app.call_from_thread(callback)


__all__ = ["MessageController"]
