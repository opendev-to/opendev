"""Context overview display for REPL."""

from typing import TYPE_CHECKING, Optional

from rich.console import Console
from rich.panel import Panel
from rich.table import Table

from swecli.repl.ui.text_utils import truncate_text

if TYPE_CHECKING:
    from swecli.core.runtime import ModeManager, ConfigManager
    from swecli.core.context_engineering.history import SessionManager
    from swecli.ui_textual.components import NotificationCenter


class ContextDisplay:
    """Renders compact context sidebar showing system state."""

    def __init__(
        self,
        console: Console,
        mode_manager: "ModeManager",
        session_manager: "SessionManager",
        config_manager: "ConfigManager",
        notification_center: "NotificationCenter",
    ):
        """Initialize context display.

        Args:
            console: Rich console for output
            mode_manager: Mode manager for current mode
            session_manager: Session manager for token tracking
            config_manager: Config manager for settings
            notification_center: Notification center for recent alerts
        """
        self.console = console
        self.mode_manager = mode_manager
        self.session_manager = session_manager
        self.config_manager = config_manager
        self.notification_center = notification_center

    def render(
        self,
        last_prompt: str = "",
        last_operation_summary: str = "—",
        last_error: Optional[str] = None,
    ) -> None:
        """Render a compact context sidebar above the prompt.

        Args:
            last_prompt: Last user prompt
            last_operation_summary: Summary of last operation
            last_error: Last error message (if any)
        """
        table = Table.grid(padding=(0, 1))
        table.add_column(style="dim")
        table.add_column()

        config = self.config_manager.get_config()

        table.add_row("Mode", self.mode_manager.current_mode.value.upper())
        table.add_row("Model", config.model)
        table.add_row("Workspace", str(self.config_manager.working_dir))

        total_tokens = (
            self.session_manager.current_session.total_tokens()
            if self.session_manager.current_session
            else 0
        )
        table.add_row(
            "Tokens",
            f"{total_tokens}/{config.max_context_tokens}",
        )

        if last_prompt:
            table.add_row("Last Prompt", truncate_text(last_prompt, 80))

        if last_operation_summary and last_operation_summary != "—":
            table.add_row("Last Tool", truncate_text(last_operation_summary, 80))

        if last_error:
            table.add_row("Last Error", f"[red]{truncate_text(last_error, 80)}[/red]")

        if self.notification_center.has_items():
            table.add_row(
                "Notifications",
                ", ".join(
                    truncate_text(note.summary(), 50)
                    for note in self.notification_center.latest(2)
                ),
            )

        panel = Panel(table, title="Context", border_style="cyan")
        self.console.print(panel)
