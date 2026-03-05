"""Controller for inline approval prompts within the Textual chat app."""

from __future__ import annotations

import asyncio
from typing import Any, Optional, TYPE_CHECKING

from rich.console import Group
from rich.panel import Panel
from rich.text import Text

if TYPE_CHECKING:
    from swecli.ui_textual.managers.interrupt_manager import InterruptManager


class ApprovalPromptController:
    """Encapsulates the inline approval prompt state machine."""

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
        self._future: asyncio.Future[tuple[bool, str, str]] | None = None
        self._options: list[dict[str, Any]] = []
        self._selected_index = 0
        self._command: str = ""
        self._working_dir: str = "."

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def active(self) -> bool:
        return self._active

    async def start(self, command: str, working_dir: str) -> tuple[bool, str, str]:
        """Display the approval prompt and wait for a choice."""

        if self._future is not None:
            raise RuntimeError("Approval prompt already active")

        self._command = command or ""
        self._working_dir = working_dir or "."
        base_prefix = ""
        if self._command.strip():
            base_prefix = self._command.strip().split()[0]
        auto_desc = (
            f"Automatically approve commands starting with '{base_prefix}' in {self._working_dir}."
            if base_prefix
            else f"Automatically approve future commands in {self._working_dir}."
        )
        self._options = [
            {"choice": "1", "label": "Yes", "description": "Run this command now.", "approved": True},
            {
                "choice": "2",
                "label": "Yes, and don't ask again",
                "description": auto_desc,
                "approved": True,
            },
            {
                "choice": "3",
                "label": "No",
                "description": "Cancel and adjust your request.",
                "approved": False,
            },
        ]
        self._selected_index = 0
        self._active = True

        # Track state for interrupt handling
        if self._interrupt_manager:
            from swecli.ui_textual.managers.interrupt_manager import InterruptState
            self._interrupt_manager.enter_state(
                InterruptState.APPROVAL_PROMPT,
                controller_ref=self,
            )

        loop = asyncio.get_running_loop()
        self._future = loop.create_future()

        self.app.input_field.load_text("")

        controller = getattr(self.app, "_autocomplete_controller", None)
        if controller is not None:
            controller.reset()

        conversation = self.app.conversation
        if getattr(conversation, "_tool_call_start", None) is not None:
            timer = getattr(conversation, "_tool_spinner_timer", None)
            if timer is not None:
                timer.stop()
                conversation._tool_spinner_timer = None
            conversation._spinner_active = False
            conversation._replace_tool_call_line("⏺")

        # Clear tips from spinner service before showing approval modal
        if hasattr(self.app, "spinner_service"):
            self.app.spinner_service.clear_all_tips()

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
        if not self._active or not self._options:
            return
        self._selected_index = (self._selected_index + delta) % len(self._options)
        self._render()

    def confirm(self) -> None:
        if not self._active or not self._future or self._future.done():
            return

        option = self._options[self._selected_index]
        edited_command = self.app.input_field.text.strip() or self._command
        result = (option.get("approved", True), option["choice"], edited_command)

        conversation = self.app.conversation
        call_display = getattr(conversation, "_tool_display", None)
        call_start = getattr(conversation, "_tool_call_start", None)

        conversation.clear_approval_prompt()

        if option.get("approved", True):
            if call_display is not None:
                conversation.start_tool_execution()
        else:
            # Always show interrupt message for consistent UX
            if call_start is not None:
                # Update the tool line in-place with red bullet
                timer = getattr(conversation, "_tool_spinner_timer", None)
                if timer is not None:
                    timer.stop()
                    conversation._tool_spinner_timer = None
                conversation._spinner_active = False
                conversation._replace_tool_call_line("⏺", success=False)

            # Always show interrupt message, regardless of tool call state
            from swecli.ui_textual.utils.interrupt_utils import create_interrupt_text, APPROVAL_INTERRUPT_MESSAGE
            result_line = create_interrupt_text(APPROVAL_INTERRUPT_MESSAGE)
            conversation.write(result_line, scroll_end=True, animate=False)
            conversation.write(Text(""))

            # Clear tool state
            conversation._tool_display = None
            conversation._tool_call_start = None

        self._future.set_result(result)

    def cancel(self) -> None:
        if not self._active or not self._options:
            return
        self._selected_index = len(self._options) - 1
        self._render()
        self.confirm()

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _cleanup(self) -> None:
        # Exit state tracking
        if self._interrupt_manager:
            self._interrupt_manager.exit_state()

        conversation = self.app.conversation
        conversation.clear_approval_prompt()
        self._future = None
        self._active = False
        self._options = []
        self._selected_index = 0
        controller = getattr(self.app, "_autocomplete_controller", None)
        if controller is not None:
            controller.reset()
        self.app.input_field.focus()
        self.app.input_field.load_text("")

    def _render(self) -> None:
        if not self._active:
            return

        cmd_display = self._command or "(empty command)"

        header = Text.assemble(
            ("Command", "dim"),
            (" · ", "dim"),
            (cmd_display, "bold bright_cyan"),
        )
        location = Text.assemble(
            ("Directory", "dim"),
            (" · ", "dim"),
            (self._working_dir, "dim"),
        )
        hint = Text("↑/↓ choose · Enter confirm · Esc cancel", style="dim")

        option_lines: list[Text] = []
        for index, option in enumerate(self._options):
            is_active = index == self._selected_index
            pointer = "▸" if is_active else " "
            pointer_style = "bright_cyan" if is_active else "dim"
            label_style = "bold white" if is_active else "white"
            desc_style = "dim"

            line = Text()
            line.append(pointer, style=pointer_style)
            line.append(f" {option['choice']}. ", style="dim")
            line.append(option["label"], style=label_style)
            description = option.get("description", "")
            if description:
                line.append(" — ", style="dim")
                line.append(description, style=desc_style)
            option_lines.append(line)

        body = Group(header, location, hint, Text(""), *option_lines)

        panel = Panel(
            body,
            title="Approval",
            border_style="bright_cyan",
            padding=(1, 2),
        )

        conversation = self.app.conversation
        conversation.render_approval_prompt([panel])
        conversation.scroll_end(animate=False)
