"""Main output formatter that delegates to specialized formatters."""

from typing import Dict, Any, Union
from rich.console import Console
from rich.panel import Panel

from swecli.ui_textual.formatters.style_formatter import StyleFormatter
from .formatter_base import TOOL_ICONS
from .file_formatters import FileFormatter
from .directory_formatter import DirectoryFormatter
from .bash_formatter import BashFormatter
from .generic_formatter import GenericFormatter


class OutputFormatter:
    """Formats tool outputs with Claude Code styling."""

    def __init__(self, console: Console, use_claude_style: bool = True):
        """Initialize output formatter.

        Args:
            console: Rich console for output
            use_claude_style: Whether to use Claude Code style (default: True)
        """
        self.console = console
        self.use_claude_style = use_claude_style

        # Initialize Claude-style formatter (new style)
        if self.use_claude_style:
            self.claude_formatter = StyleFormatter()
        else:
            # Initialize legacy formatters (old style)
            self.file_formatter = FileFormatter(console)
            self.directory_formatter = DirectoryFormatter(console)
            self.plan_formatter = PlanFormatter(console)
            self.bash_formatter = BashFormatter(console)
            self.generic_formatter = GenericFormatter(console)

    def format_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Union[str, Panel]:
        """Format a tool result.

        Args:
            tool_name: Name of the tool
            tool_args: Tool arguments
            result: Tool execution result

        Returns:
            Formatted string (Claude style) or Panel (legacy style)
        """
        # Use new Claude Code style if enabled
        if self.use_claude_style:
            return self.claude_formatter.format_tool_result(tool_name, tool_args, result)

        # Fall back to legacy panel style
        # Get tool icon
        icon = TOOL_ICONS.get(tool_name, "⏺")

        # Format based on tool type

        if tool_name == "write_file":
            return self.file_formatter.format_write_file(icon, tool_args, result)
        elif tool_name == "edit_file":
            return self.file_formatter.format_edit_file(icon, tool_args, result)
        elif tool_name == "read_file":
            return self.file_formatter.format_read_file(icon, tool_args, result)
        elif tool_name == "list_directory":
            return self.directory_formatter.format_list_directory(icon, tool_args, result)
        elif tool_name == "bash_execute":
            return self.bash_formatter.format_bash_execute(icon, tool_args, result)
        else:
            return self.generic_formatter.format_generic(icon, tool_name, tool_args, result)
