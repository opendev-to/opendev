"""Spinner controller for the Textual chat app."""

from __future__ import annotations

import random
from typing import Optional, TYPE_CHECKING

from rich.text import Text

from swecli.ui_textual.style_tokens import GREY, BLUE_BRIGHT

if TYPE_CHECKING:  # pragma: no cover
    from swecli.ui_textual.chat_app import SWECLIChatApp
    from swecli.ui_textual.components import TipsManager
    from swecli.core.context_engineering.tools.todo_handler import TodoHandler


class SpinnerController:
    """Manages the in-conversation spinner.

    This controller delegates animation to ConversationLog which uses
    widget-level timers for reliable animation. The controller's job is to:
    - Determine the spinner message (from todo or random thinking verb)
    - Get tips from TipsManager
    - Coordinate start/stop with the conversation widget
    """

    def __init__(
        self,
        app: "SWECLIChatApp",
        tips_manager: "TipsManager",
        todo_handler: Optional["TodoHandler"] = None,
    ) -> None:
        self.app = app
        self.tips_manager = tips_manager
        self.todo_handler = todo_handler
        self._message = "Thinking…"
        self._active = False
        self._current_tip = ""

    @property
    def active(self) -> bool:
        return self._active

    def start(self, message: Optional[str] = None) -> None:
        """Start the thinking spinner.

        Args:
            message: Optional custom message. If not provided, uses todo's
                     activeForm or a random thinking verb.
        """
        if self._active:
            return

        conversation = getattr(self.app, "conversation", None)
        if conversation is None:
            return

        if message is not None:
            self._message = message
        else:
            # Try to get activeForm from current in_progress todo
            active_todo_msg = None
            if self.todo_handler:
                active_todo_msg = self.todo_handler.get_active_todo_message()
            if active_todo_msg:
                self._message = active_todo_msg
            else:
                self._message = self._default_message()

        self._active = True
        self._current_tip = self.tips_manager.get_next_tip() if self.tips_manager else ""

        # Build the initial message with tip
        text = self._format_spinner_text()

        # Delegate to conversation - it handles its own animation
        conversation.start_spinner(text)

    def stop(self) -> None:
        """Stop the thinking spinner."""
        if not self._active:
            return

        self._active = False
        self._current_tip = ""

        conversation = getattr(self.app, "conversation", None)
        if conversation is not None:
            conversation.stop_spinner()

    def resume(self) -> None:
        """Resume the spinner if not active."""
        if not self._active:
            self.start(self._default_message())

    def _format_spinner_text(self) -> Text:
        """Format the spinner text with message and tip.

        Note: Frame character and timing are handled by ConversationLog.
        We just provide the message and tip.
        """
        renderable = Text()
        # Just provide the message - ConversationLog adds spinner char and timing
        renderable.append(self._message, style=BLUE_BRIGHT)

        if self._current_tip:
            renderable.append("\n")
            renderable.append("  ⎿  Tip: ", style=GREY)
            renderable.append(self._current_tip, style=GREY)

        return renderable

    @staticmethod
    def _default_message() -> str:
        try:
            from swecli.repl.llm_caller import LLMCaller

            verb = random.choice(LLMCaller.THINKING_VERBS)
        except Exception:
            verb = "Thinking"
        return f"{verb}…"
