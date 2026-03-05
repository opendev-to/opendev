from typing import Optional

from rich.console import Group
from rich.panel import Panel
from rich.text import Text

from swecli.ui_textual import style_tokens

class DisplayFormatter:
    """Formats general UI messages like errors, info, and warnings."""

    def format_error(self, primary: str, secondary: Optional[str] = None) -> Text:
        """Formats an error message."""
        text = Text.from_markup(f"  [{style_tokens.ERROR}]⎿  {primary}[/{style_tokens.ERROR}]")
        if secondary:
            text.append("\n  ⎿  ")
            text.append(secondary, style=style_tokens.SUBTLE)
        return text

    def format_info(self, primary: str, secondary: Optional[str] = None) -> Text:
        """Formats an info message."""
        text = Text.from_markup(f"  ⎿  {primary}")
        if secondary:
            text.append("\n  ⎿  ")
            text.append(secondary, style=style_tokens.SUBTLE)
        return text

    def format_warning(self, primary: str, secondary: Optional[str] = None) -> Text:
        """Formats a warning message."""
        text = Text.from_markup(f"  [{style_tokens.WARNING}]⎿  {primary}[/{style_tokens.WARNING}]")
        if secondary:
            text.append("\n  ⎿  ")
            text.append(secondary, style=style_tokens.SUBTLE)
        return text

    def format_usage(self, primary: str, secondary: Optional[str] = None, title: Optional[str] = None) -> Panel:
        """Formats a usage/help message inside a bordered panel."""
        lines = primary.strip("\n").splitlines()
        renderables: list[Text] = []

        for raw in lines:
            stripped = raw.rstrip()
            if not stripped:
                renderables.append(Text(""))
                continue

            lower = stripped.lower()
            if lower.startswith("usage:"):
                usage = stripped[len("usage:"):].strip()
                line = Text()
                line.append("Usage", style=f"bold {style_tokens.PRIMARY}")
                if usage:
                    line.append(": ", style=f"bold {style_tokens.PRIMARY}")
                    line.append(usage, style=f"bold {style_tokens.ACCENT}")
                renderables.append(line)
                continue

            if lower.startswith("key subcommands"):
                line = Text(stripped, style=f"bold {style_tokens.PRIMARY}")
                renderables.append(line)
                continue

            if lower.startswith("more:"):
                line = Text(stripped, style=style_tokens.SUBTLE)
                renderables.append(line)
                continue

            if raw.startswith("  "):
                # Likely a subcommand entry: "sub - description"
                parts = stripped.split("-", 1)
                line = Text()
                line.append(parts[0].strip(), style="bold " + style_tokens.ACCENT)
                if len(parts) > 1:
                    line.append("  ")
                    line.append(parts[1].strip(), style=style_tokens.SUBTLE)
                renderables.append(line)
                continue

            renderables.append(Text(stripped, style=style_tokens.PRIMARY))

        if secondary:
            renderables.append(Text(secondary, style=style_tokens.SUBTLE))

        panel_title = title or "Command"
        return Panel(
            Group(*renderables),
            border_style=style_tokens.ACCENT,
            padding=(0, 1),
            title=panel_title,
        )
