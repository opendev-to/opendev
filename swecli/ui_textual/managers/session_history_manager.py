"""Session history management for the Textual chat app."""

from __future__ import annotations

from typing import Callable, Optional, TYPE_CHECKING

from swecli.models.message import ChatMessage, Role

if TYPE_CHECKING:  # pragma: no cover
    from swecli.ui_textual.chat_app import SWECLIChatApp
    from swecli.core.context_engineering.history import SessionManager


class SessionHistoryManager:
    """Manage session history hydration and persistence."""

    def __init__(
        self,
        app: "SWECLIChatApp",
        session_manager: "SessionManager",
        render_stored_tool_calls_callback: Callable,
    ) -> None:
        """Initialize the session history manager.

        Args:
            app: The Textual chat application
            session_manager: The session manager instance
            render_stored_tool_calls_callback: Callback to render tool calls
        """
        self.app = app
        self.session_manager = session_manager
        self._render_stored_tool_calls_callback = render_stored_tool_calls_callback
        self._initial_messages: list[ChatMessage] = []
        self._history_restored = False

    def snapshot_session_history(self) -> list[ChatMessage]:
        """Capture a copy of existing session messages for later hydration.

        Returns:
            List of copied chat messages
        """
        if self.session_manager is None:
            return []
        session = self.session_manager.get_current_session()
        if session is None or not session.messages:
            return []
        return [message.model_copy(deep=True) for message in session.messages]

    def set_initial_messages(self, messages: list[ChatMessage]) -> None:
        """Set the initial messages for hydration.

        Args:
            messages: List of initial chat messages
        """
        self._initial_messages = messages

    def wrap_on_ready_callback(
        self,
        downstream: Optional[Callable[[], None]],
    ) -> Callable[[], None]:
        """Ensure history hydration runs before any existing on_ready hook.

        Args:
            downstream: The downstream callback to run after hydration

        Returns:
            Wrapped callback function
        """
        def _callback() -> None:
            self.hydrate_conversation_history()
            if downstream:
                downstream()

        return _callback

    def hydrate_conversation_history(self) -> None:
        """Replay the persisted session transcript into the Textual conversation log."""
        if self._history_restored:
            return

        if not self._initial_messages:
            self._history_restored = True
            return

        conversation = getattr(self.app, "conversation", None)
        if conversation is None:
            return

        conversation.clear()
        history = getattr(self.app, "_history", None)
        record_assistant = getattr(self.app, "record_assistant_message", None)

        for message in self._initial_messages:
            content = (message.content or "").strip()
            if message.role == Role.USER:
                if not content:
                    continue
                conversation.add_user_message(content)
                if history is not None and hasattr(history, "record"):
                    history.record(content)
            elif message.role == Role.ASSISTANT:
                if content:
                    conversation.add_assistant_message(content)
                    if callable(record_assistant):
                        record_assistant(content)
                if getattr(message, "tool_calls", None):
                    self._render_stored_tool_calls_callback(conversation, message.tool_calls)
                elif not content:
                    continue
            elif message.role == Role.SYSTEM:
                if not content:
                    continue
                conversation.add_system_message(content)

        self._history_restored = True


__all__ = ["SessionHistoryManager"]
