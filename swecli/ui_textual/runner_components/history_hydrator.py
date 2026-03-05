"""History hydration component for TextualRunner.

This module handles replaying persisted session transcripts into the Textual
conversation log when resuming or continuing sessions.
"""

from __future__ import annotations

import logging
import threading
import time
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable, Optional

logger = logging.getLogger(__name__)

if TYPE_CHECKING:
    from swecli.core.context_engineering.history import SessionManager
    from swecli.models.message import ChatMessage

from swecli.models.message import Role


class HistoryHydrator:
    """Manages hydration of conversation history from persisted sessions.

    This class encapsulates all logic for capturing session history snapshots
    and replaying them into the Textual conversation log, supporting both
    synchronous and asynchronous (batched) hydration strategies.

    Attributes:
        working_dir: The working directory for path resolution.

    Example:
        >>> hydrator = HistoryHydrator(session_manager, working_dir)
        >>> messages = hydrator.snapshot_history()
        >>> hydrator.start_async_hydration(app, tool_renderer)
    """

    # Batch size for async hydration to keep UI responsive
    BATCH_SIZE = 5
    # Delay between batches in seconds
    BATCH_DELAY = 0.01

    def __init__(
        self,
        session_manager: SessionManager,
        working_dir: Path,
        tool_renderer: Optional[Any] = None,
    ) -> None:
        """Initialize the HistoryHydrator.

        Args:
            session_manager: The session manager to read history from.
            working_dir: Working directory for path resolution.
            tool_renderer: Optional ToolRenderer for rendering stored tool calls.
        """
        self._session_manager = session_manager
        self._working_dir = working_dir
        self._tool_renderer = tool_renderer
        self._ledger: Any = None
        self._initial_messages: list[ChatMessage] = []
        self._history_restored = False

    @property
    def is_restored(self) -> bool:
        """Return True if history has been fully restored."""
        return self._history_restored

    @property
    def initial_messages(self) -> list[ChatMessage]:
        """Return the captured initial messages snapshot."""
        return self._initial_messages

    def snapshot_history(self) -> list[ChatMessage]:
        """Capture a copy of existing session messages for later hydration.

        Returns:
            A deep copy of the current session messages, or empty list if none.
        """
        session = self._session_manager.get_current_session()
        if session is None or not session.messages:
            return []
        self._initial_messages = [message.model_copy(deep=True) for message in session.messages]
        return self._initial_messages

    def set_tool_renderer(self, tool_renderer: Any) -> None:
        """Set the tool renderer for rendering stored tool calls.

        Args:
            tool_renderer: The ToolRenderer instance to use.
        """
        self._tool_renderer = tool_renderer

    def start_async_hydration(
        self,
        app: Any,
        on_complete: Optional[Callable[[], None]] = None,
    ) -> None:
        """Start hydrating conversation history in background batches.

        This method runs hydration in a worker thread to avoid blocking the
        UI thread. Messages are processed in batches with small delays between
        them to keep the UI responsive.

        Args:
            app: The Textual app instance with conversation widget.
            on_complete: Optional callback to invoke when hydration completes.
        """
        if self._history_restored or not self._initial_messages:
            self._history_restored = True
            if on_complete:
                on_complete()
            return

        def hydrate_in_batches() -> None:
            try:
                conversation = getattr(app, "conversation", None)
                if conversation is None:
                    logger.warning("Conversation widget not found during history hydration")
                    return

                # Clear and prepare - do this on UI thread
                try:
                    app.call_from_thread(conversation.clear)
                except Exception as e:
                    logger.error(f"Failed to clear conversation: {e}")
                    return

                history = getattr(app, "_history", None)
                record_assistant = getattr(app, "record_assistant_message", None)

                # Process messages in batches to keep UI responsive
                messages = self._initial_messages

                for i in range(0, len(messages), self.BATCH_SIZE):
                    batch = messages[i : i + self.BATCH_SIZE]

                    # Process batch on UI thread
                    def process_batch(batch_messages: list[ChatMessage] = batch) -> None:
                        for message in batch_messages:
                            try:
                                self._hydrate_single_message(
                                    message,
                                    conversation,
                                    history,
                                    record_assistant,
                                )
                            except Exception as e:
                                logger.error(f"Failed to hydrate message: {e}")

                    try:
                        app.call_from_thread(process_batch)
                    except Exception as e:
                        logger.error(f"Failed to process batch on UI thread: {e}")

                    # Small delay between batches to let UI breathe
                    time.sleep(self.BATCH_DELAY)

            except Exception as e:
                logger.error(f"History hydration failed: {e}")
            finally:
                self._history_restored = True
                if on_complete:
                    try:
                        app.call_from_thread(on_complete)
                    except Exception as e:
                        logger.error(f"Failed to call on_complete callback: {e}")

        thread = threading.Thread(target=hydrate_in_batches, daemon=True)
        thread.start()

    def hydrate_sync(self, app: Any) -> None:
        """Synchronously replay persisted session transcript into conversation log.

        This is a blocking operation that should only be used when async
        hydration is not needed (e.g., small history or testing).

        Args:
            app: The Textual app instance with conversation widget.
        """
        if self._history_restored:
            return

        if not self._initial_messages:
            self._history_restored = True
            return

        conversation = getattr(app, "conversation", None)
        if conversation is None:
            return

        conversation.clear()
        history = getattr(app, "_history", None)
        record_assistant = getattr(app, "record_assistant_message", None)

        for message in self._initial_messages:
            self._hydrate_single_message(
                message,
                conversation,
                history,
                record_assistant,
            )

        self._history_restored = True

    def _hydrate_single_message(
        self,
        message: ChatMessage,
        conversation: Any,
        history: Any,
        record_assistant: Optional[Callable[[str], None]],
    ) -> None:
        """Hydrate a single message into the conversation log.

        Args:
            message: The message to hydrate.
            conversation: The conversation widget.
            history: Optional history recorder.
            record_assistant: Optional callback for recording assistant messages.
        """
        content = (message.content or "").strip()

        if message.role == Role.USER:
            if not content:
                return
            conversation.add_user_message(content)
            # Register with ledger for cross-path dedup (if available)
            if self._ledger is not None:
                self._ledger.replay_message("user", content)
            if history is not None and hasattr(history, "record"):
                history.record(content)

        elif message.role == Role.ASSISTANT:
            # Display thinking trace first (if present)
            thinking_trace = getattr(message, "thinking_trace", None)
            if thinking_trace and hasattr(conversation, "add_thinking_block"):
                conversation.add_thinking_block(thinking_trace)

            # Display reasoning content (for o1/o3 models)
            reasoning_content = getattr(message, "reasoning_content", None)
            if reasoning_content and hasattr(conversation, "add_thinking_block"):
                conversation.add_thinking_block(reasoning_content)

            if content:
                conversation.add_assistant_message(content)
                # Register with ledger for cross-path dedup (if available)
                if self._ledger is not None:
                    self._ledger.replay_message("assistant", content)
                if callable(record_assistant):
                    record_assistant(content)

            # Render tool calls using the tool renderer if available
            tool_calls = getattr(message, "tool_calls", None)
            if tool_calls and self._tool_renderer is not None:
                self._tool_renderer.render_stored_tool_calls(conversation, tool_calls)

            # Only return early if truly nothing to display
            # (no content, no thinking, no reasoning, AND no tool calls)
            if not content and not thinking_trace and not reasoning_content and not tool_calls:
                return

        elif message.role == Role.SYSTEM:
            if not content:
                return
            conversation.add_system_message(content)

    def wrap_on_ready_callback(
        self,
        app: Any,
        downstream: Optional[Callable[[], None]],
    ) -> Callable[[], None]:
        """Create a callback that runs history hydration before downstream hook.

        Args:
            app: The Textual app instance.
            downstream: Optional existing on_ready callback to chain.

        Returns:
            A wrapped callback that hydrates history first.
        """

        def _callback() -> None:
            # Start async history hydration in background - don't block UI
            self.start_async_hydration(app)
            if downstream:
                downstream()

        return _callback
