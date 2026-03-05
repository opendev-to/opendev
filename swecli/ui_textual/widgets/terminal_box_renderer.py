"""Unified terminal box renderer for VS Code-style terminal output."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass
from typing import Callable, List

from rich.text import Text

from swecli.ui_textual.style_tokens import (
    BLUE_PATH,
    ERROR,
    GREEN_PROMPT,
    GREY,
    PANEL_BORDER,
    SUBTLE,
)
from swecli.ui_textual.utils.output_summarizer import summarize_output, get_expansion_hint


@dataclass
class TerminalBoxConfig:
    """Configuration for a terminal box."""

    command: str = ""
    working_dir: str = "."
    depth: int = 0
    is_error: bool = False
    box_width: int = 80


class TerminalBoxRenderer:
    """Renders VS Code-style terminal boxes with consistent styling.

    Supports both complete (all-at-once) and streaming (incremental) rendering.
    All methods return Text objects - caller controls when/how to write them.
    """

    # Color constants for terminal box elements
    POINTER_COLOR = GREY
    PATH_COLOR = BLUE_PATH
    PROMPT_COLOR = GREEN_PROMPT
    COMMAND_COLOR = GREY
    ERROR_COLOR = ERROR
    BORDER_DEFAULT = PANEL_BORDER
    BORDER_ERROR = ERROR

    # Truncation settings by depth (head + tail lines shown)
    MAIN_AGENT_HEAD_LINES = 3
    MAIN_AGENT_TAIL_LINES = 2
    SUBAGENT_HEAD_LINES = 2
    SUBAGENT_TAIL_LINES = 1

    def __init__(self, width_provider: Callable[[], int]):
        """Initialize renderer.

        Args:
            width_provider: Callable that returns the current box width
        """
        self._get_width = width_provider

    # --- Utility methods ---

    @staticmethod
    def format_path(path: str) -> str:
        """Shorten path by replacing home directory with ~."""
        home = os.path.expanduser("~")
        if path.startswith(home):
            return "~" + path[len(home) :]
        return path

    @staticmethod
    def normalize_line(line: str) -> str:
        """Normalize: expand tabs, strip ANSI codes."""
        line = line.expandtabs(4)
        line = re.sub(r"\x1b\[[0-9;]*m", "", line)
        return line

    def _get_border_color(self, config: TerminalBoxConfig) -> str:
        """Get border color based on error state."""
        return self.BORDER_ERROR if config.is_error else self.BORDER_DEFAULT

    def _get_indent(self, config: TerminalBoxConfig) -> str:
        """Get indentation string based on depth."""
        return "  " * config.depth

    def _get_content_width(self, box_width: int) -> int:
        """Get content width (space for text inside borders)."""
        return box_width - 5  # Space for "│  " (3) + " │" (2)

    @staticmethod
    def truncate_lines_head_tail(
        lines: List[str],
        head_count: int,
        tail_count: int,
    ) -> tuple:
        """Split lines into head, tail, and count of hidden lines.

        Args:
            lines: All output lines
            head_count: Number of lines to keep from start
            tail_count: Number of lines to keep from end

        Returns:
            Tuple of (head_lines, tail_lines, hidden_count)
            If no truncation needed, returns (lines, [], 0)
        """
        total = len(lines)
        max_lines = head_count + tail_count

        if total <= max_lines:
            return (lines, [], 0)

        head = lines[:head_count]
        tail = lines[-tail_count:]
        hidden = total - max_lines

        return (head, tail, hidden)

    # --- Line rendering primitives ---

    def render_top_border(self, config: TerminalBoxConfig) -> Text:
        """Render the top border line: ⎿ ╭────────╮"""
        indent = self._get_indent(config)
        border = self._get_border_color(config)
        box_width = config.box_width

        top = Text(f"{indent}  \u23bf ", style=self.POINTER_COLOR)
        top.append("\u256d" + "\u2500" * (box_width - 2) + "\u256e", style=border)
        return top

    def render_padding_line(self, config: TerminalBoxConfig) -> Text:
        """Render an empty padding line: │          │"""
        indent = self._get_indent(config)
        border = self._get_border_color(config)
        box_width = config.box_width

        padding = Text(f"{indent}    ")
        padding.append("\u2502" + " " * (box_width - 2) + "\u2502", style=border)
        return padding

    def render_prompt_line(self, config: TerminalBoxConfig) -> Text:
        """Render the prompt line: │  ~/path $ command  │"""
        indent = self._get_indent(config)
        border = self._get_border_color(config)
        box_width = config.box_width
        content_width = self._get_content_width(box_width)

        formatted_path = self.format_path(config.working_dir)
        prompt_prefix_len = len(formatted_path) + 3  # path + " $ "
        max_cmd_len = content_width - prompt_prefix_len

        # Normalize command - replace newlines with spaces for single-line display
        cmd_normalized = config.command.replace("\n", " ").replace("  ", " ").strip()
        cmd_display = cmd_normalized[:max_cmd_len] if max_cmd_len > 0 else ""

        prompt_line = Text(f"{indent}    ")
        prompt_line.append("\u2502  ", style=border)
        prompt_line.append(formatted_path, style=self.PATH_COLOR)
        prompt_line.append(" $ ", style=self.PROMPT_COLOR)
        prompt_line.append(cmd_display, style=self.COMMAND_COLOR)

        # Pad to EXACT content_width for aligned right border
        total_content = prompt_prefix_len + len(cmd_display)
        padding_needed = content_width - total_content
        prompt_line.append(" " * max(0, padding_needed))
        prompt_line.append(" \u2502", style=border)
        return prompt_line

    def render_content_line(
        self,
        line: str,
        config: TerminalBoxConfig,
        apply_error_style: bool = False,
    ) -> Text:
        """Render a content line: │  output text  │

        Args:
            line: The text content to render
            config: Box configuration
            apply_error_style: Whether to apply error styling to text
        """
        indent = self._get_indent(config)
        border = self._get_border_color(config)
        box_width = config.box_width
        content_width = self._get_content_width(box_width)

        normalized = self.normalize_line(line)
        display = normalized[:content_width]  # Truncate if needed

        content_line = Text(f"{indent}    ")
        content_line.append("\u2502  ", style=border)

        if apply_error_style:
            content_line.append(display, style=self.ERROR_COLOR)
        else:
            content_line.append(display, style=self.POINTER_COLOR)  # Grey to match results

        # Pad to EXACT content_width for aligned right border
        padding_needed = content_width - len(display)
        content_line.append(" " * max(0, padding_needed))
        content_line.append(" \u2502", style=border)
        return content_line

    def render_bottom_border(self, config: TerminalBoxConfig) -> Text:
        """Render the bottom border line: ╰────────╯"""
        indent = self._get_indent(config)
        border = self._get_border_color(config)
        box_width = config.box_width

        bottom = Text(f"{indent}    ")
        bottom.append("\u2570" + "\u2500" * (box_width - 2) + "\u256f", style=border)
        return bottom

    def render_truncation_separator(
        self,
        hidden_count: int,
        config: TerminalBoxConfig,
    ) -> Text:
        """Render truncation separator: │  ... 42 lines hidden ...  │

        Args:
            hidden_count: Number of lines hidden
            config: Box configuration

        Returns:
            Text object for the separator line
        """
        indent = self._get_indent(config)
        border = self._get_border_color(config)
        box_width = config.box_width
        content_width = self._get_content_width(box_width)

        # Build message
        message = f"... {hidden_count} lines hidden ..."

        # Center the message
        padding_total = content_width - len(message)
        left_pad = padding_total // 2
        right_pad = padding_total - left_pad

        separator = Text(f"{indent}    ")
        separator.append("\u2502  ", style=border)
        separator.append(" " * left_pad)
        separator.append(message, style=f"{SUBTLE} italic")
        separator.append(" " * right_pad)
        separator.append(" \u2502", style=border)

        return separator

    # --- Code block rendering (minimal header bar style) ---

    def render_code_block_header(self, language: str, box_width: int) -> Text:
        """Render code block top border with language: ╭─ python ────────╮"""
        border = self.BORDER_DEFAULT
        lang_color = GREY

        # Build: ╭─ python ─────────────────╮
        # Format: "    ╭─ {language} ─...─╮"
        prefix = "\u256d\u2500 "  # ╭─
        suffix = " \u2500"  # space + ─ before the rest

        # Calculate how many dashes we need
        lang_display = language if language else "code"
        remaining = box_width - len(prefix) - len(lang_display) - len(suffix) - 1  # -1 for ╮
        dashes = "\u2500" * max(0, remaining)

        header = Text("    ")
        header.append(prefix, style=border)
        header.append(lang_display, style=lang_color)
        header.append(suffix + dashes + "\u256e", style=border)
        return header

    def render_code_block_line(self, line: str, box_width: int) -> Text:
        """Render a code block content line: │ code here  │"""
        border = self.BORDER_DEFAULT
        content_width = box_width - 4  # Space for "│ " (2) + " │" (2)

        normalized = self.normalize_line(line)
        display = normalized[:content_width]  # Truncate if needed

        content_line = Text("    ")
        content_line.append("\u2502 ", style=border)
        content_line.append(display)

        # Pad to EXACT content_width for aligned right border
        padding_needed = content_width - len(display)
        content_line.append(" " * max(0, padding_needed))
        content_line.append(" \u2502", style=border)
        return content_line

    def render_code_block_bottom(self, box_width: int) -> Text:
        """Render code block bottom border: ╰────────────╯"""
        border = self.BORDER_DEFAULT
        bottom = Text("    ")
        bottom.append("\u2570" + "\u2500" * (box_width - 2) + "\u256f", style=border)
        return bottom

    def render_code_block(
        self,
        lines: List[str],
        language: str,
        box_width: int,
    ) -> List[Text]:
        """Render a code block with minimal header bar style.

        Args:
            lines: List of code lines to display
            language: Programming language for the header
            box_width: Width of the box

        Returns:
            List of Text objects to be written sequentially.
        """
        result: List[Text] = []

        # Top border with language
        result.append(self.render_code_block_header(language, box_width))

        # Content lines
        for line in lines:
            result.append(self.render_code_block_line(line, box_width))

        # Bottom border
        result.append(self.render_code_block_bottom(box_width))

        return result

    # --- Complete box rendering ---

    def render_complete_box(
        self,
        output_lines: List[str],
        config: TerminalBoxConfig,
    ) -> List[Text]:
        """Render a complete terminal box with head+tail truncation.

        Applies truncation based on config.depth:
        - depth=0 (main agent): 10 lines max (5 head + 5 tail)
        - depth>0 (subagent): 6 lines max (3 head + 3 tail)

        Args:
            output_lines: List of output lines to display
            config: Box configuration

        Returns:
            List of Text objects to be written sequentially.
        """
        result: List[Text] = []

        # Top border
        result.append(self.render_top_border(config))

        # Top padding
        result.append(self.render_padding_line(config))

        # Prompt line
        result.append(self.render_prompt_line(config))

        # Determine truncation limits based on depth
        if config.depth == 0:
            head_count = self.MAIN_AGENT_HEAD_LINES
            tail_count = self.MAIN_AGENT_TAIL_LINES
        else:
            head_count = self.SUBAGENT_HEAD_LINES
            tail_count = self.SUBAGENT_TAIL_LINES

        # Apply truncation
        head_lines, tail_lines, hidden_count = self.truncate_lines_head_tail(
            output_lines, head_count, tail_count
        )

        # Render head lines
        for line in head_lines:
            result.append(self.render_content_line(line, config, apply_error_style=config.is_error))

        # Render separator if truncated
        if hidden_count > 0:
            result.append(self.render_truncation_separator(hidden_count, config))

        # Render tail lines
        for line in tail_lines:
            result.append(self.render_content_line(line, config, apply_error_style=config.is_error))

        # Bottom padding
        result.append(self.render_padding_line(config))

        # Bottom border
        result.append(self.render_bottom_border(config))

        return result

    # --- Collapsible output rendering ---

    def render_collapsed_summary(
        self,
        output_lines: List[str],
        depth: int = 0,
        output_type: str = "bash",
    ) -> Text:
        """Render a collapsed summary line for long output.

        Shows an intelligent summary with expansion hint instead of full content.

        Args:
            output_lines: Full output lines to summarize.
            depth: Nesting depth for indentation.
            output_type: Type of output for smarter summaries.

        Returns:
            Text object containing the summary line.
        """
        indent = "  " * depth
        summary = summarize_output(output_lines, output_type)
        hint = get_expansion_hint()

        # Build summary line: "⎿  {summary} (ctrl+o to expand)"
        result = Text(f"{indent}  \u23bf  ", style=GREY)
        result.append(summary, style=SUBTLE)
        result.append(f" {hint}", style=f"{SUBTLE} italic")

        return result

    def should_collapse_output(self, line_count: int, depth: int = 0) -> bool:
        """Determine if output should be shown collapsed.

        Args:
            line_count: Number of output lines.
            depth: Nesting depth (affects threshold).

        Returns:
            True if output should be collapsed by default.
        """
        # Collapse threshold: 10 lines for main agent, 6 for subagents
        if depth == 0:
            threshold = self.MAIN_AGENT_HEAD_LINES + self.MAIN_AGENT_TAIL_LINES
        else:
            threshold = self.SUBAGENT_HEAD_LINES + self.SUBAGENT_TAIL_LINES

        return line_count > threshold
