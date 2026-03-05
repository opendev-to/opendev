"""Textual modal for command approval prompts."""

from __future__ import annotations

from typing import Tuple

from rich.text import Text
from textual.app import ComposeResult
from textual.containers import Container, Horizontal
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Static


class CommandApprovalModal(ModalScreen[Tuple[bool, str, str]]):
    """Modal dialog to approve or edit shell commands."""

    DEFAULT_CSS = """
    CommandApprovalModal > Container {
        width: 60;
        border: round $accent;
        background: $surface;
        padding: 1 2;
    }

    CommandApprovalModal .title {
        content-align: center middle;
        height: 1;
        color: $accent;
    }

    CommandApprovalModal .command {
        background: $boost;
        color: $text;
        padding: 1;
        border: round $accent;
    }

    CommandApprovalModal Input {
        margin-top: 1;
    }

    CommandApprovalModal Horizontal {
        margin-top: 1;
        height: 3;
        content-align: center middle;
    }

    CommandApprovalModal Button {
        min-width: 16;
        margin: 0 1;
    }
    """

    def __init__(self, command: str, working_dir: str) -> None:
        super().__init__()
        try:
            self._command = Text.from_markup(command or "").plain
        except Exception:
            self._command = command or ""
        self._working_dir = working_dir
        self._input: Input | None = None

    def compose(self) -> ComposeResult:
        command_block = Text(self._command or "", style="cyan")
        with Container():
            yield Static("Confirm Bash Command", classes="title")
            yield Static(f"Working directory: [green]{self._working_dir or '.'}[/]", markup=True)
            yield Static(command_block, classes="command")
            self._input = Input(
                value=self._command or "", placeholder="Edit command before running..."
            )
            yield self._input
            with Horizontal():
                yield Button("Run command", id="approve")
                yield Button("Run and approve similar", id="approve_all")
                yield Button("Cancel", id="deny", variant="error")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if not self._input:
            edited_command = self._command or ""
        else:
            edited_command = self._input.value.strip()

        if event.button.id == "approve":
            self.dismiss((True, "1", edited_command))
        elif event.button.id == "approve_all":
            self.dismiss((True, "2", edited_command))
        elif event.button.id == "deny":
            self.dismiss((False, "3", edited_command))

    def on_input_submitted(self, event: Input.Submitted) -> None:
        self.dismiss((True, "1", event.value.strip()))

    def on_mount(self) -> None:
        if self._input:
            self._input.focus()
