"""Panel components for plugin installation wizard using Rich components."""

from rich.console import Group, RenderableType
from rich.panel import Panel
from rich.table import Table
from rich.text import Text

from swecli.ui_textual.style_tokens import BLUE_BG_ACTIVE, BLUE_LIGHT, GREY


def create_scope_selection_panel(selected_index: int, working_dir: str = "") -> RenderableType:
    """Create scope selection panel for plugin installation.

    Args:
        selected_index: Currently selected option (0=User, 1=Project)
        working_dir: Working directory path for display in project option

    Returns:
        Rich Panel renderable
    """
    # Build project path label - show actual path if available
    if working_dir:
        project_label = f"Project ({working_dir}/.opendev/plugins/)"
    else:
        project_label = "Project (.opendev/plugins/)"

    items = [
        {
            "option": "1",
            "label": "User (~/.opendev/plugins/)",
            "summary": "Available in all projects",
        },
        {
            "option": "2",
            "label": project_label,
            "summary": "Only in current project",
        },
    ]

    table = Table.grid(expand=False, padding=(0, 1))
    table.add_column(width=2, justify="center")  # Pointer
    table.add_column(width=7, justify="center")  # Option number
    table.add_column(ratio=1)  # Label
    table.add_column(ratio=1)  # Summary

    for row_index, item in enumerate(items):
        is_active = row_index == selected_index
        pointer = "❯" if is_active else " "
        row_style = f"on {BLUE_BG_ACTIVE}" if is_active else ""
        pointer_style = "bold bright_cyan" if is_active else "dim"
        label_style = "bold white" if is_active else "white"
        summary_style = "dim white" if is_active else "dim"
        option_style = "bold bright_cyan" if is_active else "dim"
        table.add_row(
            Text(pointer, style=pointer_style),
            Text(item["option"], style=option_style),
            Text(item["label"], style=label_style),
            Text(item["summary"], style=summary_style),
            style=row_style,
        )

    instructions = Text(
        "Enter 1 or 2 to select scope, then press Enter.",
        style=f"italic {GREY}",
    )
    header = Text("Choose installation scope for this plugin.", style=BLUE_LIGHT)

    return Panel(
        Group(header, table, instructions),
        title="[bold]Installation Scope[/bold]",
        title_align="left",
        border_style="bright_cyan",
        padding=(1, 2),
    )
