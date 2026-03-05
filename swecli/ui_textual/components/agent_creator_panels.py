"""Panel messages for agent creation wizard using Rich components."""

from rich.console import Group, RenderableType
from rich.panel import Panel
from rich.table import Table
from rich.text import Text

from swecli.ui_textual.style_tokens import BLUE_BG_ACTIVE, BLUE_LIGHT, GREY

# Maximum number of tools to show at once
_MAX_VISIBLE_TOOLS = 10


def create_location_panel(selected_index: int, working_dir: str = "") -> RenderableType:
    """Create location selection panel using Rich components.

    Args:
        selected_index: Currently selected option (0=Project, 1=Personal)
        working_dir: Working directory path for display in project option

    Returns:
        Rich Panel renderable
    """
    # Build project path label - show actual path if available
    if working_dir:
        project_label = f"Project ({working_dir}/.opendev/agents/)"
    else:
        project_label = "Project (.opendev/agents/)"

    items = [
        {
            "option": "1",
            "label": project_label,
            "summary": "Local to this repository",
        },
        {"option": "2", "label": "Personal (~/.opendev/agents/)", "summary": "Available everywhere"},
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
        "Use ↑/↓ or 1-2 to select, Enter to confirm, Esc to cancel.",
        style=f"italic {GREY}",
    )
    header = Text("Choose where to create the agent.", style=BLUE_LIGHT)

    return Panel(
        Group(header, table, instructions),
        title="[bold]Create New Agent[/bold]",
        title_align="left",
        border_style="bright_cyan",
        padding=(1, 2),
    )


def create_method_panel(selected_index: int) -> RenderableType:
    """Create creation method selection panel using Rich components.

    Args:
        selected_index: Currently selected option (0=Generate, 1=Manual, 2=Back)

    Returns:
        Rich Panel renderable
    """
    items = [
        {
            "option": "1",
            "label": "Generate with AI",
            "summary": "AI creates the agent definition",
        },
        {
            "option": "2",
            "label": "Manual configuration",
            "summary": "Write the system prompt yourself",
        },
        {"option": "B", "label": "← Back", "summary": "Return to previous step"},
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
        "Use ↑/↓ or 1-2 to select, Enter to confirm, B to go back, Esc to cancel.",
        style=f"italic {GREY}",
    )
    header = Text("How should the agent be created?", style=BLUE_LIGHT)

    return Panel(
        Group(header, table, instructions),
        title="[bold]Creation Method[/bold]",
        title_align="left",
        border_style="bright_blue",
        padding=(1, 2),
    )


def create_identifier_input_panel(current_value: str = "", error: str = "") -> RenderableType:
    """Create agent identifier input panel using Rich components.

    Args:
        current_value: Current text in input field
        error: Error message to display (if any)

    Returns:
        Rich Panel renderable
    """
    header = Text("Enter a unique identifier for your agent:", style=BLUE_LIGHT)
    hint = Text("e.g., test-runner, code-reviewer, tech-lead", style="dim")

    elements = [header, hint]

    if error:
        elements.append(Text(f"⚠ {error}", style="bold yellow"))

    # Input field visualization
    display_value = current_value if current_value else ""
    input_width = 50
    if len(display_value) > input_width:
        display_value = display_value[-(input_width - 3) :] + "..."

    input_text = Text()
    input_text.append("[", style="dim")
    input_text.append(display_value, style="bold bright_green")
    input_text.append(" " * max(0, input_width - len(display_value)), style="")
    input_text.append("]", style="dim")
    elements.append(input_text)

    instructions = Text(
        "Type in the input box below. Press Enter to confirm, Esc to cancel.",
        style=f"italic {GREY}",
    )
    elements.append(instructions)

    return Panel(
        Group(*elements),
        title="[bold]Agent Identifier[/bold]",
        title_align="left",
        border_style="bright_cyan",
        padding=(1, 2),
    )


def create_prompt_input_panel(current_value: str = "") -> RenderableType:
    """Create system prompt input panel using Rich components.

    Args:
        current_value: Current text in input field

    Returns:
        Rich Panel renderable
    """
    header = Text("Enter the system prompt for your agent:", style=BLUE_LIGHT)
    hint = Text("Be comprehensive for best results", style="dim")

    elements = [header, hint]

    # Show preview of input (first few lines)
    input_width = 50
    if current_value:
        preview_lines = current_value.split("\n")[:3]
        for pline in preview_lines:
            if len(pline) > input_width:
                pline = pline[: input_width - 3] + "..."
            input_text = Text()
            input_text.append("[", style="dim")
            input_text.append(pline, style="bold bright_green")
            input_text.append(" " * max(0, input_width - len(pline)), style="")
            input_text.append("]", style="dim")
            elements.append(input_text)
        if len(current_value.split("\n")) > 3:
            more_count = len(current_value.split("\n")) - 3
            elements.append(Text(f"... (+{more_count} more lines)", style="dim"))
    else:
        input_text = Text()
        input_text.append("[", style="dim")
        input_text.append(" " * input_width, style="")
        input_text.append("]", style="dim")
        elements.append(input_text)

    instructions = Text(
        "Type in the input box below. Press Enter to confirm, Esc to cancel.",
        style=f"italic {GREY}",
    )
    elements.append(instructions)

    return Panel(
        Group(*elements),
        title="[bold]System Prompt[/bold]",
        title_align="left",
        border_style="bright_cyan",
        padding=(1, 2),
    )


def create_description_input_panel(current_value: str = "") -> RenderableType:
    """Create description input panel for AI generation using Rich components.

    Args:
        current_value: Current text in input field

    Returns:
        Rich Panel renderable
    """
    header = Text(
        "Describe what this agent should do and when it should be used:", style=BLUE_LIGHT
    )
    hint = Text("Be comprehensive for best results", style="dim")

    elements = [header, hint]

    # Input field visualization
    input_width = 50
    if current_value:
        preview_lines = current_value.split("\n")[:3]
        for pline in preview_lines:
            if len(pline) > input_width:
                pline = pline[: input_width - 3] + "..."
            input_text = Text()
            input_text.append("[", style="dim")
            input_text.append(pline, style="bold bright_green")
            input_text.append(" " * max(0, input_width - len(pline)), style="")
            input_text.append("]", style="dim")
            elements.append(input_text)
    else:
        input_text = Text()
        input_text.append("[", style="dim")
        input_text.append(" " * input_width, style="")
        input_text.append("]", style="dim")
        elements.append(input_text)

    instructions = Text(
        "Type in the input box below. Press Enter to confirm, Esc to cancel.",
        style=f"italic {GREY}",
    )
    elements.append(instructions)

    return Panel(
        Group(*elements),
        title="[bold]Describe Your Agent[/bold]",
        title_align="left",
        border_style="bright_cyan",
        padding=(1, 2),
    )


def create_generating_panel(
    description: str = "",
    spinner_char: str = "⠋",
    elapsed_seconds: int = 0,
) -> RenderableType:
    """Panel with description and animated spinner during generation.

    Args:
        description: The description being used for generation
        spinner_char: Current spinner animation character
        elapsed_seconds: Seconds since generation started

    Returns:
        Rich Panel renderable
    """
    elements = []

    # Header
    header = Text("Creating agent based on your description:", style=BLUE_LIGHT)
    elements.append(header)

    # Show description preview (truncated)
    if description:
        preview = description[:60] + "..." if len(description) > 60 else description
        desc_text = Text()
        desc_text.append('  "', style="dim")
        desc_text.append(preview, style="italic dim white")
        desc_text.append('"', style="dim")
        elements.append(desc_text)

    elements.append(Text(""))  # Spacing

    # Animated spinner row
    spinner_row = Text()
    spinner_row.append(spinner_char, style="bright_cyan")
    spinner_row.append(f" Generating... ({elapsed_seconds}s)", style="dim")
    elements.append(spinner_row)

    return Panel(
        Group(*elements),
        title="[bold]Generating Agent[/bold]",
        title_align="left",
        border_style="bright_yellow",
        padding=(1, 2),
    )


def create_success_panel(agent_name: str, agent_path: str) -> RenderableType:
    """Create success panel after agent creation using Rich components.

    Args:
        agent_name: Name of the created agent
        agent_path: Path to the agent file

    Returns:
        Rich Panel renderable
    """
    header = Text(f"✓ Created agent: {agent_name}", style="bold bright_green")

    # Show path (potentially truncated)
    display_path = agent_path
    max_path_len = 55
    if len(display_path) > max_path_len:
        display_path = "..." + display_path[-(max_path_len - 3) :]

    location_label = Text("Location:", style="dim")
    location_path = Text(f"  {display_path}", style="white")
    instructions = Text("Use /agents list to see all agents", style=f"italic {GREY}")

    return Panel(
        Group(header, location_label, location_path, instructions),
        title="[bold]Agent Created[/bold]",
        title_align="left",
        border_style="bright_green",
        padding=(1, 2),
    )


def create_tool_selection_panel(
    tools: list,
    selected_indices: set[int],
    focused_index: int,
    scroll_offset: int = 0,
    warning: str = "",
) -> RenderableType:
    """Create tool selection panel with checkboxes.

    Args:
        tools: List of ToolInfo objects with name, display_name, description
        selected_indices: Set of currently selected tool indices
        focused_index: Index of currently focused tool
        scroll_offset: Scroll position for long lists
        warning: Optional warning message to display

    Returns:
        Rich Panel renderable with scrollable checkbox list
    """
    elements = []

    header = Text("Select tools for this agent:", style=BLUE_LIGHT)
    elements.append(header)

    if warning:
        elements.append(Text(f"⚠ {warning}", style="bold yellow"))

    elements.append(Text(""))  # Spacing

    # Calculate visible range
    total_tools = len(tools)
    visible_count = min(_MAX_VISIBLE_TOOLS, total_tools)

    # Ensure focused index is visible
    if focused_index < scroll_offset:
        scroll_offset = focused_index
    elif focused_index >= scroll_offset + visible_count:
        scroll_offset = focused_index - visible_count + 1

    # Clamp scroll offset
    scroll_offset = max(0, min(scroll_offset, total_tools - visible_count))

    # Create table for tools
    table = Table.grid(expand=False, padding=(0, 1))
    table.add_column(width=2, justify="center")  # Pointer
    table.add_column(width=4, justify="center")  # Checkbox
    table.add_column(width=20)  # Tool name
    table.add_column(ratio=1)  # Description

    # Add visible rows
    end_index = min(scroll_offset + visible_count, total_tools)
    for i in range(scroll_offset, end_index):
        tool = tools[i]
        is_selected = i in selected_indices
        is_focused = i == focused_index

        # Pointer
        pointer = "❯" if is_focused else " "

        # Checkbox with appropriate style
        if is_selected:
            checkbox = "[×]"
            checkbox_style = "bold bright_green" if is_focused else "bright_green"
        else:
            checkbox = "[ ]"
            checkbox_style = "white" if is_focused else "dim"

        # Tool name and description
        name_style = "bold white" if is_focused else "white"
        desc_style = "dim white" if is_focused else "dim"

        row_style = f"on {BLUE_BG_ACTIVE}" if is_focused else ""

        table.add_row(
            Text(pointer, style="bold bright_cyan" if is_focused else "dim"),
            Text(checkbox, style=checkbox_style),
            Text(tool.display_name, style=name_style),
            Text(tool.description, style=desc_style),
            style=row_style,
        )

    elements.append(table)

    # Scroll indicators
    scroll_text = Text()
    if scroll_offset > 0:
        scroll_text.append("▲ ", style="dim cyan")
    if end_index < total_tools:
        scroll_text.append("▼ ", style="dim cyan")
    if scroll_text.plain:
        elements.append(Text())  # Spacing
        elements.append(scroll_text)

    elements.append(Text())  # Spacing

    # Selection hints
    hints = Text()
    hints.append("[a] All ", style="bright_cyan")
    hints.append("  ", style="")
    hints.append("[n] None ", style="bright_cyan")
    hints.append("  ", style="")
    hints.append("[i] Invert", style="bright_cyan")
    elements.append(hints)

    # Instructions
    instructions = Text(
        "Use ↑/↓ to navigate, Space to toggle, Enter to confirm, Esc to cancel.",
        style=f"italic {GREY}",
    )
    elements.append(instructions)

    return Panel(
        Group(*elements),
        title="[bold]Select Tools[/bold]",
        title_align="left",
        border_style="bright_cyan",
        padding=(1, 2),
    )
