"""Message processing engine for TextualRunner.

This module handles the background message processing thread, queue management,
and orchestrating the execution of commands and queries via callbacks.
"""

from __future__ import annotations

import logging
import queue
import threading
from typing import Any, Callable, Optional

from swecli.models.message import ChatMessage

logger = logging.getLogger(__name__)


class MessageProcessor:
    """Manages the background message processing loop and queue."""

    def __init__(
        self,
        app: Any,
        callbacks: dict[str, Any],
    ) -> None:
        """Initialize the processor.

        Args:
            app: The Textual app instance.
            callbacks: Dictionary containing handler functions:
                - handle_command: Callable[[str], None]
                - handle_query: Callable[[str], list[ChatMessage]]
                - render_responses: Callable[[list[ChatMessage]], None]
                - on_error: Callable[[str], None]  # Generic errors
                - on_command_error: Callable[[str], None] # Command-specific errors
        """
        self._app = app
        self._callbacks = callbacks

        # Queue holds tuples of (message, needs_display)
        self._pending: queue.Queue[tuple[str, bool]] = queue.Queue()

        self._processor_thread: threading.Thread | None = None
        self._processor_stop = threading.Event()
        self._message_ready = threading.Event()  # Signal when message is enqueued
        self._paused = threading.Event()  # Set when paused

        # Live message injection support
        self._injection_target: Optional[Callable[[str], None]] = None
        self._injection_queue_ref = None  # Reference to ReactExecutor._injection_queue
        self._injecting_lock = threading.Lock()

        # Callback to update UI queue indicator
        self._queue_update_callback: Callable[[int], None] | None = None
        if hasattr(app, "update_queue_indicator"):
            self._queue_update_callback = app.update_queue_indicator

    def set_app(self, app: Any) -> None:
        """Set the Textual app instance."""
        self._app = app
        if hasattr(app, "update_queue_indicator"):
            self._queue_update_callback = app.update_queue_indicator

    def get_queue_size(self) -> int:
        """Get number of messages waiting in queue (including injection queue)."""
        size = self._pending.qsize()
        with self._injecting_lock:
            q = self._injection_queue_ref
        if q is not None:
            size += q.qsize()
        return size

    def pause(self) -> None:
        """Pause message processing. Messages stay queued."""
        self._paused.set()

    def resume(self) -> None:
        """Resume message processing."""
        self._paused.clear()
        self._message_ready.set()  # Wake up to process any waiting messages

    def is_paused(self) -> bool:
        """Check if processor is paused."""
        return self._paused.is_set()

    def set_injection_target(
        self, callback: Optional[Callable[[str], None]], injection_queue=None
    ) -> None:
        """Set or clear the live injection target.

        When set, non-command messages are forwarded to the callback instead
        of being queued for sequential processing.
        """
        with self._injecting_lock:
            self._injection_target = callback
            self._injection_queue_ref = injection_queue

    def enqueue_message(self, text: str, needs_display: bool = False) -> None:
        """Queue a message for processing.

        If a live injection target is active and the message is not a slash
        command, the message is injected directly into the running agent loop
        and displayed immediately in the UI.

        Args:
            text: The message text.
            needs_display: Whether to display the message in the UI when processing starts.
        """
        # Check if we should inject into the running agent loop
        with self._injecting_lock:
            target = self._injection_target
        if target is not None and not text.startswith("/"):
            target(text)
            # Display is deferred to when _drain_injected_messages() consumes
            # the message at a ReAct step boundary (via _on_message_consumed).
            # Update the queue indicator so user sees "N queued".
            self._notify_queue_update(from_ui_thread=True)
            return

        item = (text, needs_display)
        self._pending.put_nowait(item)
        self._message_ready.set()  # Wake up processor immediately
        self._notify_queue_update(from_ui_thread=True)

    def start(self) -> None:
        """Start the background processor thread."""
        if self._processor_thread is not None:
            return

        self._processor_stop.clear()
        self._processor_thread = threading.Thread(
            target=self._run_loop, daemon=True, name="message-processor"
        )
        self._processor_thread.start()

    def stop(self) -> None:
        """Stop the background processor thread."""
        if self._processor_thread is not None:
            self._processor_stop.set()
            self._message_ready.set()  # Wake up thread so it can exit
            self._processor_thread.join(timeout=2.0)
            self._processor_thread = None

    def _notify_queue_update(self, from_ui_thread: bool = False) -> None:
        """Notify UI of queue size change."""
        if not self._queue_update_callback:
            return

        size = self.get_queue_size()
        if from_ui_thread:
            self._queue_update_callback(size)
        else:
            self._app.call_from_thread(self._queue_update_callback, size)

    def _run_loop(self) -> None:
        """Main processing loop running in background thread."""
        while not self._processor_stop.is_set():
            try:
                # Wait for message signal or periodic check for stop
                self._message_ready.wait(timeout=0.5)
                self._message_ready.clear()

                try:
                    message, needs_display = self._pending.get_nowait()
                except queue.Empty:
                    continue

                # Check if paused - put message back and wait
                if self._paused.is_set():
                    self._pending.put((message, needs_display))
                    continue

                # Update indicator to show waiting count (excluding current)
                self._notify_queue_update(from_ui_thread=False)

                is_command = message.startswith("/")

                # Start local spinner for non-commands
                if not is_command and hasattr(self._app, "_start_local_spinner"):
                    self._app.call_from_thread(self._app._start_local_spinner)

                # Display user message if needed (queued while busy)
                if needs_display and not is_command:
                    ledger = getattr(self._app, "_display_ledger", None)
                    if ledger:
                        ledger.display_user_message(
                            message,
                            "message_processor",
                            call_on_ui=self._app.call_from_thread,
                        )
                    else:
                        logger.warning(
                            "DisplayLedger not available, falling back to direct "
                            "display (source=%s)", "message_processor"
                        )
                        self._app.call_from_thread(
                            self._app.conversation.add_user_message, message
                        )
                    if hasattr(self._app.conversation, "refresh"):
                        self._app.call_from_thread(self._app.conversation.refresh)

                try:
                    if is_command:
                        handler = self._callbacks.get("handle_command")
                        if handler:
                            handler(message)
                    else:
                        handler = self._callbacks.get("handle_query")
                        render = self._callbacks.get("render_responses")
                        if handler:
                            new_messages = handler(message)
                            if new_messages and render:
                                self._app.call_from_thread(render, new_messages)
                except Exception as exc:  # pragma: no cover - defensive
                    if is_command:
                        err_handler = self._callbacks.get("on_command_error")
                        if err_handler:
                            self._app.call_from_thread(err_handler, str(exc))
                    else:
                        err_handler = self._callbacks.get("on_error")
                        if err_handler:
                            self._app.call_from_thread(err_handler, str(exc))
                finally:
                    self._pending.task_done()

                    # Notify completion if queue empty (for both commands and messages)
                    if self._pending.empty():
                        if hasattr(self._app, "notify_processing_complete"):
                            self._app.call_from_thread(self._app.notify_processing_complete)

                    # Update indicator
                    self._notify_queue_update(from_ui_thread=False)

            except Exception:  # pragma: no cover - defensive
                continue
