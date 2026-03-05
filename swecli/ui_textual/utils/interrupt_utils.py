"""Utilities for handling interrupt messages across the UI."""

from rich.text import Text

from swecli.ui_textual.style_tokens import ERROR, GREY


def create_interrupt_message(message: str) -> str:
    """Create an interrupt message string with the special marker.

    This returns a string with the ::interrupted:: marker that will be
    processed by _write_generic_tool_result() in conversation_log.py to
    display with proper formatting (grey ⎿ prefix and red text).

    Args:
        message: The interrupt message content

    Returns:
        String with ::interrupted:: marker
    """
    return f"::interrupted:: {message.strip()}"


def create_interrupt_text(message: str) -> Text:
    """Create a Text object for interrupt messages with proper styling.

    This creates a Text object directly with the grey ⎿ prefix and
    bold red text formatting, bypassing the need for string markers.

    Args:
        message: The interrupt message content

    Returns:
        Text object with proper interrupt styling
    """
    line = Text("  ⎿  ", style=GREY)
    line.append(message.strip(), style=f"bold {ERROR}")
    return line


# Standard interrupt message constants
STANDARD_INTERRUPT_MESSAGE = "Interrupted · What should I do instead?"
THINKING_INTERRUPT_MESSAGE = STANDARD_INTERRUPT_MESSAGE
APPROVAL_INTERRUPT_MESSAGE = STANDARD_INTERRUPT_MESSAGE