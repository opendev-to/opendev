"""Controller for plan approval prompts within the Textual chat app."""

from __future__ import annotations

import asyncio
from typing import Any, Optional, TYPE_CHECKING

from rich.console import Group
from rich.panel import Panel
from rich.text import Text

if TYPE_CHECKING:
    from swecli.ui_textual.managers.interrupt_manager import InterruptManager


class PlanApprovalController:
    """Encapsulates the plan approval prompt state machine.

    Displays the plan in a bordered panel with three options:
    1. Start implementation (auto-approve edits)
    2. Start implementation (review edits)
    3. Revise plan
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
        self._future: asyncio.Future[dict[str, str]] | None = None
        self._options: list[dict[str, Any]] = []
        self._selected_index = 0
        self._plan_content: str = ""

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def active(self) -> bool:
        return self._active

    async def start(self, plan_content: str) -> dict[str, str]:
        """Display the plan approval panel and wait for a choice."""

        if self._future is not None:
            # Stale state from a previous interrupted call — clean up and proceed
            if not self._future.done():
                self._future.cancel()
            self._cleanup()

        self._plan_content = plan_content or ""
        self._options = [
            {
                "label": "Start implementation",
                "description": "Auto-approve file edits during implementation.",
                "action": "approve_auto",
            },
            {
                "label": "Start implementation (review edits)",
                "description": "Review each file edit before it's applied.",
                "action": "approve",
            },
            {
                "label": "Revise plan",
                "description": "Stay in plan mode and provide feedback.",
                "action": "modify",
            },
        ]
        self._selected_index = 0
        self._active = True

        # Track state for interrupt handling
        if self._interrupt_manager:
            from swecli.ui_textual.managers.interrupt_manager import InterruptState

            self._interrupt_manager.enter_state(
                InterruptState.PLAN_APPROVAL,
                controller_ref=self,
            )

        loop = asyncio.get_running_loop()
        self._future = loop.create_future()

        self.app.input_field.load_text("")

        controller = getattr(self.app, "_autocomplete_controller", None)
        if controller is not None:
            controller.reset()

        # Stop tool spinner before showing panel
        conversation = self.app.conversation
        if getattr(conversation, "_tool_call_start", None) is not None:
            timer = getattr(conversation, "_tool_spinner_timer", None)
            if timer is not None:
                timer.stop()
                conversation._tool_spinner_timer = None
            conversation._spinner_active = False
            conversation._replace_tool_call_line("⏺")

        # Clear tips from spinner service before showing panel
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
        action = option["action"]

        conversation = self.app.conversation
        conversation.clear_plan_approval_prompt()

        # Clear tool state
        conversation._tool_display = None
        conversation._tool_call_start = None

        self._future.set_result({"action": action, "feedback": ""})

    def cancel(self) -> None:
        if not self._active or not self._options:
            return
        # Default to "Revise plan" (last option)
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
        conversation.clear_plan_approval_prompt()
        self._future = None
        self._active = False
        self._options = []
        self._selected_index = 0
        self._plan_content = ""
        controller = getattr(self.app, "_autocomplete_controller", None)
        if controller is not None:
            controller.reset()
        self.app.input_field.focus()
        self.app.input_field.load_text("")

    def _render(self) -> None:
        if not self._active:
            return

        header = Text.assemble(
            ("Plan", "dim"),
            (" · ", "dim"),
            ("Ready for review", "bold bright_cyan"),
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
            line.append(f" {index + 1}. ", style="dim")
            line.append(option["label"], style=label_style)
            description = option.get("description", "")
            if description:
                line.append(" — ", style="dim")
                line.append(description, style=desc_style)
            option_lines.append(line)

        body = Group(header, hint, Text(""), *option_lines)

        panel = Panel(
            body,
            title="Approval",
            border_style="bright_cyan",
            padding=(1, 2),
        )

        conversation = self.app.conversation
        conversation.render_plan_approval_prompt([panel])
        conversation.scroll_end(animate=False)
