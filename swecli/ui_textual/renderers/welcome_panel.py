"""Helpers for rendering the welcome panel inside the conversation log."""

from __future__ import annotations

import os
from pathlib import Path
from typing import Optional

from rich.align import Align
from rich.panel import Panel
from rich.text import Text

from swecli.core.runtime import OperationMode
from swecli.ui_textual.components import WelcomeMessage
from swecli.ui_textual.style_tokens import BLUE_BRIGHT, SUBTLE, GREEN_BRIGHT, PRIMARY, CYAN


def render_welcome_panel(
    conversation,
    *,
    real_integration: bool,
    working_dir: Optional[Path] = None,
    username: Optional[str] = None,
    current_mode: OperationMode = OperationMode.NORMAL,
) -> None:
    """Render a welcome panel tailored for either real or POC integrations."""

    if real_integration:
        resolved_working_dir = working_dir or Path.cwd()
        resolved_username = username or os.getenv("USER", "Developer")

        welcome_lines = WelcomeMessage.generate_full_welcome(
            current_mode=current_mode,
            working_dir=resolved_working_dir,
            username=resolved_username,
        )

        for line in welcome_lines:
            conversation.write(Text.from_ansi(line), wrappable=False)

    else:
        heading = Text("OpenDev (Preview)", style=f"bold {BLUE_BRIGHT}")
        subheading = Text("Textual POC interface", style=SUBTLE)
        body = Text(
            "Use this playground to explore the upcoming Textual UI.\n"
            "Core flows are stubbed; use /help, /demo, or /scroll to interact."
        )

        shortcuts = Text()
        shortcuts.append("Enter", style=f"bold {GREEN_BRIGHT}")
        shortcuts.append(" send   •   ")
        shortcuts.append("Shift+Enter", style=f"bold {GREEN_BRIGHT}")
        shortcuts.append(" new line   •   ")
        shortcuts.append("/help", style=f"bold {CYAN}")
        shortcuts.append(" commands")

        content = Text.assemble(
            heading,
            "\n",
            subheading,
            "\n\n",
            body,
            "\n\n",
            shortcuts,
        )

        panel = Panel(
            Align.center(content, vertical="middle"),
            border_style=BLUE_BRIGHT,
            padding=(1, 3),
            title="Welcome",
            subtitle="opendev",
            subtitle_align="left",
            width=78,
        )

        conversation.write(panel)

    # Use direct write with wrappable=False to avoid resize re-rendering issues
    # The empty line separator should not trigger block re-rendering on terminal resize
    conversation.write(Text(""), wrappable=False)
