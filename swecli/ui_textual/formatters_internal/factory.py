"""Factory for creating appropriate tool formatters."""

from typing import Dict, Any

from rich.console import Console
from rich.panel import Panel

from .base import BaseToolFormatter
from .file_operations import WriteFileFormatter, ReadFileFormatter, EditFileFormatter
from .system_operations import BashExecuteFormatter, ListDirectoryFormatter, GenericToolFormatter


class FormatterFactory:
    """Factory for creating appropriate tool formatters."""

    def __init__(self, console: Console):
        """Initialize formatter factory.

        Args:
            console: Rich console for output
        """
        self.console = console
        self._formatters: Dict[str, BaseToolFormatter] = {
            "write_file": WriteFileFormatter(console),
            "edit_file": EditFileFormatter(console),
            "read_file": ReadFileFormatter(console),
            "list_directory": ListDirectoryFormatter(console),
            "bash_execute": BashExecuteFormatter(console),
            # Generic formatter for unknown tools
            "generic": GenericToolFormatter(console),
        }

    def format_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
    ) -> Panel:
        """Format a tool result using the appropriate formatter.

        Args:
            tool_name: Name of the tool
            tool_args: Tool arguments
            result: Tool execution result

        Returns:
            Formatted panel
        """
        # Get the appropriate formatter
        formatter = self._formatters.get(tool_name, self._formatters["generic"])
        return formatter.format(tool_name, tool_args, result)

    def register_formatter(self, tool_name: str, formatter: BaseToolFormatter) -> None:
        """Register a custom formatter for a tool.

        Args:
            tool_name: Name of the tool
            formatter: Custom formatter instance
        """
        self._formatters[tool_name] = formatter
