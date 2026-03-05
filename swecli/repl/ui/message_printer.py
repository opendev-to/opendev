"""Message printing utilities for REPL."""

from rich.console import Console
from rich.text import Text

from swecli.ui_textual.formatters_internal.markdown_formatter import markdown_to_plain_text


class MessagePrinter:
    """Handles printing of formatted messages in the REPL."""

    def __init__(self, console: Console):
        """Initialize message printer.

        Args:
            console: Rich console for output
        """
        self.console = console

    def print_markdown_message(
        self,
        content: str,
        *,
        symbol: str = "⏺",
    ) -> None:
        """Render assistant content as simple plain text with a leading symbol.

        Args:
            content: Message content (may contain markdown)
            symbol: Leading symbol to display
        """
        cleaned = (content or "").strip()
        plain = markdown_to_plain_text(cleaned) if cleaned else ""
        lines = plain.splitlines() if plain else []

        if lines:
            lines[0] = f"{symbol} {lines[0]}"
        else:
            lines = [symbol]

        message = "\n".join(lines)
        self.console.print(Text(message))
