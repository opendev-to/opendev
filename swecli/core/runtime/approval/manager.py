"""Interactive approval workflows for OpenDev."""

from __future__ import annotations

from enum import Enum
from typing import Any, Optional

from prompt_toolkit import prompt
from prompt_toolkit.application import Application
from prompt_toolkit.formatted_text import FormattedText
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.layout import Layout
from prompt_toolkit.layout.containers import Window
from prompt_toolkit.layout.controls import FormattedTextControl
from rich.console import Console
from rich.live import Live
from rich.panel import Panel
from rich.text import Text

from swecli.models.operation import Operation
from swecli.ui_textual.style_tokens import PT_META_GREY


class ApprovalChoice(str, Enum):
    """User's approval choice."""

    APPROVE = "1"
    APPROVE_ALL = "2"
    DENY = "3"
    EDIT = "e"
    QUIT = "q"


class ApprovalResult:
    """Result of an approval request."""

    def __init__(
        self,
        approved: bool,
        choice: ApprovalChoice,
        edited_content: Optional[str] = None,
        apply_to_all: bool = False,
        cancelled: bool = False,
    ) -> None:
        self.approved = approved
        self.choice = choice
        self.edited_content = edited_content
        self.apply_to_all = apply_to_all
        self.cancelled = cancelled


class ApprovalManager:
    """Manager for interactive approval workflows."""

    def __init__(self, console: Optional[Console] = None) -> None:
        self.console = console or Console()
        self.auto_approve_remaining = False
        self.approved_patterns = set()

    def _create_operation_message(self, operation: Operation, preview: str) -> str:
        op_type = operation.type.value
        preview_short = preview[:100].replace("\n", " ")
        if len(preview) > 100:
            preview_short += "..."

        if op_type == "file_write":
            return "Do you want to create/write this file?"
        if op_type == "file_edit":
            return "Do you want to edit this file?"
        if op_type == "file_delete":
            return "Do you want to delete this file?"
        if op_type == "bash_execute":
            return "Do you want to run this command?"
        return f"Do you want to approve this {op_type} operation?"

    def _show_interactive_menu(
        self,
        message: str,
        preview: str,
        command: Optional[str] = None,
        working_dir: Optional[str] = None,
    ) -> ApprovalChoice:
        selected_index = [0]

        if command and working_dir:
            cmd_name = command.split()[0] if command.split() else command
            options = [
                ("1", "Yes"),
                ("2", f"Yes, and don't ask again for {cmd_name} commands in {working_dir}"),
                ("3", "No, and tell OpenDev what to do differently", "esc"),
            ]
        else:
            options = [
                ("1", "Yes"),
                ("2", "Yes, allow all operations during this session", "shift+tab"),
                ("3", "No, and tell OpenDev what to do differently", "esc"),
            ]

        def get_formatted_text() -> FormattedText:
            lines = []
            lines.append(("", "\n"))
            lines.append(
                ("class:border", "┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓\n")
            )

            preview_lines = preview.split("\n")[:3]
            for line in preview_lines:
                truncated = line[:57] if len(line) > 57 else line
                padding = " " * (57 - len(truncated))
                lines.append(("class:preview", f"┃ {truncated}{padding}┃\n"))

            if len(preview.split("\n")) > 3:
                lines.append(
                    (
                        "class:preview",
                        "┃ ...                                                     ┃\n",
                    )
                )

            lines.append(
                ("class:border", "┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛\n")
            )
            lines.append(("", "\n"))
            lines.append(("class:question bold", f"{message}\n"))
            lines.append(("", "\n"))

            for idx, option_info in enumerate(options):
                number = option_info[0]
                text = option_info[1]
                shortcut = option_info[2] if len(option_info) > 2 else None
                cursor = "❯" if idx == selected_index[0] else " "
                option_prefix = f"  {cursor} {number}. {text}"

                if idx == selected_index[0]:
                    lines.append(("class:selected bold", option_prefix))
                    if shortcut:
                        lines.append((PT_META_GREY, f" ({shortcut})"))
                    lines.append(("", "\n"))
                else:
                    lines.append(("class:option", option_prefix))
                    if shortcut:
                        lines.append((PT_META_GREY, f" ({shortcut})"))
                    lines.append(("", "\n"))

            return FormattedText(lines)

        kb = KeyBindings()

        @kb.add("up")
        def _up(event):  # type: ignore[unused-ignore]
            selected_index[0] = max(0, selected_index[0] - 1)

        @kb.add("down")
        def _down(event):  # type: ignore[unused-ignore]
            selected_index[0] = min(len(options) - 1, selected_index[0] + 1)

        @kb.add("enter")
        def _enter(event):  # type: ignore[unused-ignore]
            event.app.exit(result=options[selected_index[0]][0])

        @kb.add("1")
        def _select_one(event):  # type: ignore[unused-ignore]
            event.app.exit(result="1")

        @kb.add("2")
        def _select_two(event):  # type: ignore[unused-ignore]
            event.app.exit(result="2")

        @kb.add("3")
        def _select_three(event):  # type: ignore[unused-ignore]
            event.app.exit(result="3")

        @kb.add("s-tab")
        def _shift_tab(event):  # type: ignore[unused-ignore]
            event.app.exit(result="2")

        @kb.add("escape")
        def _escape(event):  # type: ignore[unused-ignore]
            event.app.exit(result="3")

        layout = Layout(
            Window(
                FormattedTextControl(get_formatted_text),
                dont_extend_height=True,
            )
        )

        app = Application(layout=layout, key_bindings=kb, full_screen=False)
        result = app.run()
        return ApprovalChoice(result)

    def _prompt_command_confirmation(
        self,
        command: str,
        working_dir: Optional[str],
    ) -> ApprovalResult:
        if not working_dir:
            working_dir = "current directory"

        message = (
            f"Allow future commands similar to '{command}' in {working_dir}?\n"
            "Enter choice: (y)es, (n)o, (a)lways, (s)kip"
        )
        choice = prompt(f"{message}\n> ")
        choice = choice.lower().strip()

        if choice in {"y", "yes"}:
            return ApprovalResult(True, ApprovalChoice.APPROVE)
        if choice in {"a", "always"}:
            return ApprovalResult(True, ApprovalChoice.APPROVE_ALL, apply_to_all=True)
        if choice in {"s", "skip"}:
            return ApprovalResult(False, ApprovalChoice.DENY, cancelled=True)
        return ApprovalResult(False, ApprovalChoice.DENY)

    def reset_auto_approve(self) -> None:
        self.auto_approve_remaining = False

    def request_approval(
        self,
        operation: Operation,
        preview: str,
        *,
        command: Optional[str] = None,
        working_dir: Optional[str] = None,
        allow_edit: bool = True,
        timeout: Union[Any, None] = None,
        force_prompt: bool = False,
    ) -> ApprovalResult:
        from swecli.core.debug import get_debug_logger

        get_debug_logger().log(
            "approval_request",
            "approval",
            command=command,
            operation=operation.type.value if operation else None,
        )

        if self.auto_approve_remaining and not force_prompt:
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="auto"
            )
            return ApprovalResult(True, ApprovalChoice.APPROVE)

        message = self._create_operation_message(operation, preview)
        choice = self._show_interactive_menu(message, preview, command, working_dir)

        if choice == ApprovalChoice.APPROVE:
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="user", choice="approve"
            )
            return ApprovalResult(True, choice)

        if choice == ApprovalChoice.APPROVE_ALL:
            self.auto_approve_remaining = True
            if command and working_dir:
                self.approved_patterns.add((command, working_dir))
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="user", choice="approve_all"
            )
            return ApprovalResult(True, choice, apply_to_all=True)

        if choice == ApprovalChoice.DENY:
            get_debug_logger().log(
                "approval_result", "approval", approved=False, method="user", choice="deny"
            )
            return ApprovalResult(False, choice)

        if choice == ApprovalChoice.EDIT and allow_edit:
            edited = prompt("Enter your revised content:\n")
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="user", choice="edit"
            )
            return ApprovalResult(True, ApprovalChoice.APPROVE, edited_content=edited)

        get_debug_logger().log(
            "approval_result", "approval", approved=False, method="user", choice="deny"
        )
        return ApprovalResult(False, ApprovalChoice.DENY)

    def render_operation_preview(self, operation: Operation, preview: str) -> None:
        with Live(refresh_per_second=4):
            panel = Panel(Text(preview), title=f"Operation preview: {operation.type.value}")
            self.console.print(panel)
