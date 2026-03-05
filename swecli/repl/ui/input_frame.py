"""Input frame rendering for REPL."""

from rich.console import Console


class InputFrame:
    """Renders decorative frame around input prompt."""

    def __init__(self, console: Console):
        """Initialize input frame.

        Args:
            console: Rich console for output
        """
        self.console = console

    def print_top(self) -> None:
        """Render the top border for the input frame."""
        width = max(20, self.console.width)
        line = "╭" + "─" * max(2, width - 2) + "╮"
        self.console.print(line, style="dim")

    def print_bottom(self) -> None:
        """Render the bottom border for the input frame."""
        width = max(20, self.console.width)
        line = "╰" + "─" * max(2, width - 2) + "╯"
        self.console.print(line, style="dim")
