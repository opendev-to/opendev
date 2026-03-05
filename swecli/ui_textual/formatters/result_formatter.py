"""Centralized tool result formatting.

This module provides the ONLY API for displaying tool results with consistent
formatting. All tool output should go through ToolResultFormatter to ensure:
- Standard indentation (`  ⎿  ` prefix)
- Consistent pass/fail indicators
- Uniform color scheme from style_tokens

Usage:
    formatter = ToolResultFormatter()
    
    # Display success result
    text = formatter.format_success("File saved successfully")
    console.print(text)
    
    # Display error result
    text = formatter.format_error("File not found")
    console.print(text)
    
    # Display with secondary info
    text = formatter.format_result("Compiled", ResultType.SUCCESS, secondary="5 files")
    console.print(text)
"""

from __future__ import annotations

from enum import Enum
from typing import Optional

from rich.text import Text

from swecli.ui_textual import style_tokens


class ResultType(Enum):
    """Type of result to display."""
    SUCCESS = "success"
    ERROR = "error"
    WARNING = "warning"
    INFO = "info"


# Standard prefix used for all result lines (2-space indent + arrow + 2-space)
RESULT_PREFIX = "  ⎿  "
# Continuation prefix for multi-line messages (aligns with content after ⎿)
RESULT_CONTINUATION = "     "


class ToolResultFormatter:
    """Centralized formatter for tool result display.
    
    This is the standard API for displaying tool results. All new code
    should use this class rather than constructing result strings manually.
    
    Format pattern:
        ⏺ Tool Name (params)      <- Header (handled by CommandHandler.print_command_header)
          ⎿  Result message       <- Result line (this class)
    """
    
    def format_result(
        self,
        message: str,
        result_type: ResultType = ResultType.SUCCESS,
        secondary: Optional[str] = None,
        show_icon: bool = False,
    ) -> Text:
        """Format a tool result line with proper styling.
        
        Args:
            message: Primary result message
            result_type: Type of result (success, error, warning, info)
            secondary: Optional secondary/detail message (dimmed)
            show_icon: Whether to show ✓/✖/⚠ icon before message
            
        Returns:
            Rich Text object ready for console.print()
        """
        # Get style based on result type
        style, icon = self._get_style_and_icon(result_type)

        # Build the text - handle multi-line messages
        lines = message.split('\n')
        text = Text(RESULT_PREFIX, style=style_tokens.GREY)

        if show_icon:
            text.append(f"{icon} ", style=style)

        # First line
        text.append(lines[0], style=style)

        # Continuation lines with proper indentation
        for line in lines[1:]:
            text.append(f"\n{RESULT_CONTINUATION}", style=style_tokens.GREY)
            text.append(line, style=style)

        if secondary:
            text.append(f"\n{RESULT_PREFIX}", style=style_tokens.GREY)
            text.append(secondary, style=style_tokens.SUBTLE)

        return text
    
    def format_success(self, message: str, secondary: Optional[str] = None) -> Text:
        """Format a success result.
        
        Args:
            message: Success message
            secondary: Optional detail (dimmed)
            
        Returns:
            Rich Text with green styling
        """
        return self.format_result(message, ResultType.SUCCESS, secondary)
    
    def format_error(self, message: str, secondary: Optional[str] = None) -> Text:
        """Format an error result.
        
        Args:
            message: Error message
            secondary: Optional detail (dimmed)
            
        Returns:
            Rich Text with red styling
        """
        return self.format_result(message, ResultType.ERROR, secondary)
    
    def format_warning(self, message: str, secondary: Optional[str] = None) -> Text:
        """Format a warning result.
        
        Args:
            message: Warning message
            secondary: Optional detail (dimmed)
            
        Returns:
            Rich Text with yellow styling
        """
        return self.format_result(message, ResultType.WARNING, secondary)
    
    def format_info(self, message: str, secondary: Optional[str] = None) -> Text:
        """Format an info result (neutral styling).
        
        Args:
            message: Info message
            secondary: Optional detail (dimmed)
            
        Returns:
            Rich Text with default styling
        """
        return self.format_result(message, ResultType.INFO, secondary)
    
    def _get_style_and_icon(self, result_type: ResultType) -> tuple[str, str]:
        """Get Rich style string and icon for result type.
        
        Args:
            result_type: The type of result
            
        Returns:
            Tuple of (style_string, icon_char)
        """
        if result_type == ResultType.SUCCESS:
            return style_tokens.SUCCESS, style_tokens.SUCCESS_ICON
        elif result_type == ResultType.ERROR:
            return style_tokens.ERROR, style_tokens.ERROR_ICON
        elif result_type == ResultType.WARNING:
            return style_tokens.WARNING, style_tokens.WARNING_ICON
        else:  # INFO
            return style_tokens.PRIMARY, style_tokens.HINT_ICON


# Singleton instance for convenience
_default_formatter: Optional[ToolResultFormatter] = None


def get_formatter() -> ToolResultFormatter:
    """Get the default ToolResultFormatter instance.
    
    Returns:
        Singleton ToolResultFormatter
    """
    global _default_formatter
    if _default_formatter is None:
        _default_formatter = ToolResultFormatter()
    return _default_formatter


# Convenience functions for quick access
def format_success(message: str, secondary: Optional[str] = None) -> Text:
    """Format a success result (convenience function)."""
    return get_formatter().format_success(message, secondary)


def format_error(message: str, secondary: Optional[str] = None) -> Text:
    """Format an error result (convenience function)."""
    return get_formatter().format_error(message, secondary)


def format_warning(message: str, secondary: Optional[str] = None) -> Text:
    """Format a warning result (convenience function)."""
    return get_formatter().format_warning(message, secondary)


def format_info(message: str, secondary: Optional[str] = None) -> Text:
    """Format an info result (convenience function)."""
    return get_formatter().format_info(message, secondary)


__all__ = [
    "ResultType",
    "ToolResultFormatter",
    "RESULT_PREFIX",
    "get_formatter",
    "format_success",
    "format_error",
    "format_warning",
    "format_info",
]
