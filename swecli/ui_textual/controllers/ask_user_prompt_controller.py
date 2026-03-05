"""Controller for ask-user prompts within the Textual chat app."""

from __future__ import annotations

import asyncio
from typing import Any, Optional, TYPE_CHECKING

from rich.console import Group
from rich.panel import Panel
from rich.text import Text

from swecli.core.context_engineering.tools.implementations.ask_user_tool import (
    Question,
    QuestionOption,
)

if TYPE_CHECKING:
    from swecli.ui_textual.managers.interrupt_manager import InterruptManager


class AskUserPromptController:
    """Encapsulates the ask-user prompt state machine.

    Handles displaying questions with multiple-choice options and collecting
    user responses. Supports both single-select and multi-select questions,
    along with a custom "Other" option.
    """

    def __init__(
        self,
        app: "SWECLIChatApp",
        interrupt_manager: Optional["InterruptManager"] = None,
    ) -> None:
        if TYPE_CHECKING:  # pragma: no cover
            pass

        self.app = app
        self._interrupt_manager = interrupt_manager
        self._active = False
        self._future: asyncio.Future[dict[str, Any] | None] | None = None
        self._questions: list[Question] = []
        self._current_question_idx = 0
        self._selected_index = 0
        self._answers: dict[str, Any] = {}
        # For multi-select, track which options are selected
        self._multi_selections: set[int] = set()
        # For "Other" custom input mode
        self._other_mode = False

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def active(self) -> bool:
        return self._active

    @property
    def _is_submit_step(self) -> bool:
        """Check if we're on the final Submit step."""
        return self._current_question_idx == len(self._questions)

    async def start(self, questions: list[Question]) -> dict[str, Any] | None:
        """Display the ask-user prompt and wait for user responses.

        Args:
            questions: List of Question objects to ask

        Returns:
            Dictionary mapping question index to selected answer(s),
            or None if user cancelled/skipped
        """
        if self._future is not None:
            raise RuntimeError("Ask-user prompt already active")

        if not questions:
            return None

        self._questions = questions
        self._current_question_idx = 0
        self._selected_index = 0
        self._answers = {}
        self._multi_selections = set()
        self._other_mode = False
        self._active = True

        # Track state for interrupt handling
        if self._interrupt_manager:
            from swecli.ui_textual.managers.interrupt_manager import InterruptState
            self._interrupt_manager.enter_state(
                InterruptState.ASK_USER_PROMPT,
                controller_ref=self,
            )

        loop = asyncio.get_running_loop()
        self._future = loop.create_future()

        # Clear input field
        self.app.input_field.load_text("")

        # Reset autocomplete
        controller = getattr(self.app, "_autocomplete_controller", None)
        if controller is not None:
            controller.reset()

        # Stop active SpinnerService spinners (replaces old _tool_call_start approach)
        # Use success=True so the ask_user tool bullet stays green (not red).
        # The tool is working as intended — it's asking the user, not failing.
        if hasattr(self.app, "spinner_service"):
            spinner_service = self.app.spinner_service
            spinner_service.stop_all(immediate=True, success=True)
            spinner_service.clear_all_tips()

        self._render()
        self.app.input_field.focus()

        try:
            result = await self._future
        finally:
            self._cleanup()

        return result

    def render(self) -> None:
        """Re-render the prompt if it is active."""
        self._render()

    def move(self, delta: int) -> None:
        """Move selection up or down."""
        if not self._active:
            return

        if self._other_mode:
            # In Other mode, don't navigate - let user type
            return

        if self._is_submit_step:
            # Only 2 options: Submit (0) and Cancel (1)
            self._selected_index = (self._selected_index + delta) % 2
            self._render()
            return

        current_q = self._questions[self._current_question_idx]
        num_options = len(current_q.options) + 1  # +1 for "Other"

        self._selected_index = (self._selected_index + delta) % num_options
        self._render()

    def toggle_selection(self) -> None:
        """Toggle selection for multi-select questions (Space key)."""
        if not self._active:
            return

        if self._is_submit_step:
            return  # No multi-select on Submit step

        current_q = self._questions[self._current_question_idx]
        if not current_q.multi_select:
            return

        # Don't toggle "Other" option via space (use Enter for that)
        if self._selected_index == len(current_q.options):
            return

        if self._selected_index in self._multi_selections:
            self._multi_selections.discard(self._selected_index)
        else:
            self._multi_selections.add(self._selected_index)
        self._render()

    def confirm(self) -> None:
        """Confirm current selection."""
        if not self._active or not self._future or self._future.done():
            return

        # Handle Submit step
        if self._is_submit_step:
            conversation = self.app.conversation
            conversation.clear_ask_user_prompt()
            if self._selected_index == 0:  # Submit
                self._future.set_result(self._answers if self._answers else None)
            else:  # Cancel
                self._future.set_result(None)
            return

        current_q = self._questions[self._current_question_idx]
        is_other = self._selected_index == len(current_q.options)

        # Handle "Other" option
        if is_other and not self._other_mode:
            # Enter Other mode - user can type custom answer
            self._other_mode = True
            self.app.input_field.load_text("")
            self._render()
            return

        # If in Other mode, capture the custom input
        if self._other_mode:
            custom_text = self.app.input_field.text.strip()
            if not custom_text:
                # Empty input - stay in Other mode
                return
            if current_q.multi_select:
                # Add custom to multi selections
                existing = list(self._answers.get(str(self._current_question_idx), []))
                existing.append(custom_text)
                self._answers[str(self._current_question_idx)] = existing
            else:
                self._answers[str(self._current_question_idx)] = custom_text
            self._other_mode = False
            self.app.input_field.load_text("")
        else:
            # Normal option selection
            if current_q.multi_select:
                # Collect all selected options
                selected = []
                for idx in sorted(self._multi_selections):
                    if idx < len(current_q.options):
                        selected.append(current_q.options[idx].label)
                if selected:
                    self._answers[str(self._current_question_idx)] = selected
                else:
                    # No selections - treat as skip for this question
                    pass
            else:
                # Single select
                if self._selected_index < len(current_q.options):
                    self._answers[str(self._current_question_idx)] = current_q.options[
                        self._selected_index
                    ].label

        # Move to next question or Submit step
        self._current_question_idx += 1
        self._selected_index = 0
        self._multi_selections = set()
        self._render()  # This will render Submit step if idx == len(questions)

    def cancel(self) -> None:
        """Cancel/skip the ask-user prompt."""
        if not self._active or not self._future or self._future.done():
            return

        # If in Other mode, exit Other mode first
        if self._other_mode:
            self._other_mode = False
            self.app.input_field.load_text("")
            self._render()
            return

        conversation = self.app.conversation
        conversation.clear_ask_user_prompt()
        self._future.set_result(None)

    def handle_input(self, text: str) -> bool:
        """Handle text input when in Other mode.

        Returns True if input was handled, False otherwise.
        """
        if not self._active or not self._other_mode:
            return False
        # Input is captured in the input field, confirm() will read it
        return True

    def update_input_preview(self, text: str) -> None:
        """Update the panel to show live preview of typed text."""
        if not self._active or not self._other_mode:
            return
        self._render()

    def go_back(self) -> None:
        """Navigate to previous question."""
        if not self._active or self._current_question_idx == 0:
            return

        if self._other_mode:
            self._other_mode = False
            self.app.input_field.load_text("")

        self._current_question_idx -= 1
        self._selected_index = 0
        self._multi_selections = set()

        # Only restore multi-selections if going back to a question (not from Submit)
        if self._current_question_idx < len(self._questions):
            current_q = self._questions[self._current_question_idx]
            if current_q.multi_select:
                existing = self._answers.get(str(self._current_question_idx), [])
                if isinstance(existing, list):
                    for answer in existing:
                        for idx, opt in enumerate(current_q.options):
                            if opt.label == answer:
                                self._multi_selections.add(idx)

        self._render()

    def go_forward(self) -> None:
        """Navigate to next question or Submit step if current is answered."""
        if not self._active:
            return
        if self._is_submit_step:
            return  # Already at Submit, can't go further
        if str(self._current_question_idx) not in self._answers:
            return

        if self._other_mode:
            self._other_mode = False
            self.app.input_field.load_text("")

        # Allow moving to Submit step (idx can be len(questions))
        self._current_question_idx += 1
        self._selected_index = 0
        self._multi_selections = set()
        self._render()

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _cleanup(self) -> None:
        # Exit state tracking
        if self._interrupt_manager:
            self._interrupt_manager.exit_state()

        conversation = self.app.conversation
        conversation.clear_ask_user_prompt()
        self._future = None
        self._active = False
        self._questions = []
        self._current_question_idx = 0
        self._selected_index = 0
        self._answers = {}
        self._multi_selections = set()
        self._other_mode = False
        controller = getattr(self.app, "_autocomplete_controller", None)
        if controller is not None:
            controller.reset()
        self.app.input_field.focus()
        self.app.input_field.load_text("")

    def _render_step_bar(self) -> Text:
        """Render the step navigation bar showing progress."""
        step_bar = Text()

        # Left arrow
        can_go_back = self._current_question_idx > 0
        step_bar.append("← ", style="bright_cyan" if can_go_back else "dim")

        # Each question step
        for idx, q in enumerate(self._questions):
            is_current = idx == self._current_question_idx
            is_completed = str(idx) in self._answers

            if is_completed and not is_current:
                icon = "✔"
                icon_style = "bright_green"
            elif is_current:
                icon = "●"
                icon_style = "bright_cyan"
            else:
                icon = "☐"
                icon_style = "dim"

            label = q.header if q.header else f"Q{idx + 1}"
            step_bar.append(icon, style=icon_style)
            step_bar.append(" ", style="")
            step_bar.append(label, style="bold white" if is_current else "dim")
            step_bar.append("  ", style="")

        # Submit indicator - show as current if on submit step
        is_submit_current = self._is_submit_step
        all_answered = len(self._answers) == len(self._questions)

        if is_submit_current:
            icon = "●"
            icon_style = "bright_cyan"
            label_style = "bold white"
        elif all_answered:
            icon = "✔"
            icon_style = "bright_green"
            label_style = "dim"
        else:
            icon = "☐"
            icon_style = "dim"
            label_style = "dim"

        step_bar.append(icon, style=icon_style)
        step_bar.append(" Submit", style=label_style)

        # Right arrow - can go forward to Submit if current question answered
        can_go_forward = str(
            self._current_question_idx
        ) in self._answers and self._current_question_idx < len(
            self._questions
        )  # Can go to Submit
        step_bar.append(" →", style="bright_cyan" if can_go_forward else "dim")

        return step_bar

    def _render_submit_step(self) -> None:
        """Render the final Submit confirmation step."""
        all_answered = len(self._answers) == len(self._questions)

        header = Text("Review your answers", style="bold white")

        # Status message
        if all_answered:
            status = Text("✓ All questions answered", style="bright_green")
        else:
            answered_count = len(self._answers)
            total_count = len(self._questions)
            status = Text(
                f"⚠ You have answered {answered_count}/{total_count} questions", style="yellow"
            )

        hint = Text("←/→ steps · ↑/↓ choose · Enter confirm · Esc cancel", style="dim")

        # Options: Submit / Cancel
        options = [
            ("Submit answers", "Confirm and submit your responses"),
            ("Cancel", "Discard all answers and exit"),
        ]

        option_lines = []
        for idx, (label, desc) in enumerate(options):
            is_active = idx == self._selected_index
            pointer = "▸" if is_active else " "
            line = Text()
            line.append(pointer, style="bright_cyan" if is_active else "dim")
            line.append(f" {idx + 1}. ", style="dim")
            line.append(label, style="bold white" if is_active else "white")
            line.append("\n      ", style="")
            line.append(desc, style="dim")
            option_lines.append(line)

        body_parts = [self._render_step_bar(), Text(""), header, Text(""), status, hint, Text("")]
        body_parts.extend(option_lines)

        body = Group(*body_parts)

        panel = Panel(
            body,
            title="Question",
            border_style="bright_cyan",
            padding=(1, 2),
        )

        conversation = self.app.conversation
        conversation.render_ask_user_prompt([panel])
        conversation.scroll_end(animate=False)

    def _render(self) -> None:
        if not self._active or not self._questions:
            return

        # Check if we're on the Submit step
        if self._is_submit_step:
            self._render_submit_step()
            return

        current_q = self._questions[self._current_question_idx]
        total_q = len(self._questions)

        # Build header line
        header_parts = []
        if current_q.header:
            header_parts.append((f"[{current_q.header}]", "bold bright_cyan"))
            header_parts.append(("  ", ""))
        header_parts.append((current_q.question, "white"))

        header = Text()
        for text, style in header_parts:
            header.append(text, style=style)

        # Progress indicator for multiple questions
        progress = Text()
        if total_q > 1:
            progress.append(
                f"Question {self._current_question_idx + 1}/{total_q}",
                style="dim",
            )

        # Hint line
        if self._other_mode:
            hint = Text("Type your answer · Enter to confirm · Esc to go back", style="dim")
        elif current_q.multi_select:
            hint = Text(
                "←/→ steps · ↑/↓ move · Space toggle · Enter confirm · Esc skip", style="dim"
            )
        else:
            hint = Text("←/→ steps · ↑/↓ choose · Enter confirm · Esc skip", style="dim")

        # Build option lines
        option_lines: list[Text] = []
        for idx, opt in enumerate(current_q.options):
            is_active = idx == self._selected_index
            is_multi_selected = idx in self._multi_selections

            pointer = "▸" if is_active else " "
            pointer_style = "bright_cyan" if is_active else "dim"

            # Multi-select checkbox
            if current_q.multi_select:
                checkbox = "[x]" if is_multi_selected else "[ ]"
                checkbox_style = "bright_green" if is_multi_selected else "dim"
            else:
                checkbox = ""
                checkbox_style = ""

            label_style = "bold white" if is_active else "white"
            desc_style = "dim"

            line = Text()
            line.append(pointer, style=pointer_style)
            line.append(f" {idx + 1}. ", style="dim")
            if checkbox:
                line.append(checkbox + " ", style=checkbox_style)
            line.append(opt.label, style=label_style)
            if opt.description:
                line.append("\n      ", style="")
                line.append(opt.description, style=desc_style)
            option_lines.append(line)

        # Add "Type something." option
        other_idx = len(current_q.options)
        is_other_active = other_idx == self._selected_index

        other_line = Text()
        other_pointer = "▸" if is_other_active else " "
        other_line.append(other_pointer, style="bright_cyan" if is_other_active else "dim")
        other_line.append(f" {other_idx + 1}. ", style="dim")
        other_line.append("Type something.", style="bold white" if is_other_active else "white")
        other_line.append("\n      ", style="")
        other_line.append("(Select and press Enter to type your own answer)", style="dim")
        option_lines.append(other_line)

        # If in Other mode, show input prompt with live preview
        if self._other_mode:
            input_line = Text()
            input_line.append("    Enter your answer: ", style="bright_cyan")
            # Show current input text as live preview
            current_text = (
                self.app.input_field.text.strip() if hasattr(self.app, "input_field") else ""
            )
            if current_text:
                input_line.append(current_text, style="bold white")
                input_line.append("▎", style="bright_cyan blink")  # Cursor indicator
            else:
                input_line.append("(type below)", style="dim italic")
            option_lines.append(Text(""))
            option_lines.append(input_line)

        # Assemble body
        body_parts = [self._render_step_bar(), Text("")]
        body_parts.append(header)
        if progress.plain:
            body_parts.append(progress)
        body_parts.append(hint)
        body_parts.append(Text(""))
        body_parts.extend(option_lines)

        body = Group(*body_parts)

        panel = Panel(
            body,
            title="Question",
            border_style="bright_cyan",
            padding=(1, 2),
        )

        conversation = self.app.conversation
        conversation.render_ask_user_prompt([panel])
        conversation.scroll_end(animate=False)
