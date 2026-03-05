"""Convert Rich renderables to plain text for terminal fallbacks."""

from __future__ import annotations

from io import StringIO
from typing import Any

from rich.console import Console


def rich_to_text_box(renderable: Any, width: int = 78) -> str:
    buffer = StringIO()
    console = Console(
        file=buffer,
        width=width,
        force_terminal=True,
        force_interactive=False,
        legacy_windows=False,
        markup=True,
        emoji=True,
        highlight=True,
        color_system="truecolor",
    )
    console.print(renderable)
    output = buffer.getvalue()
    if output.endswith("\n"):
        output = output[:-1]
    return output
