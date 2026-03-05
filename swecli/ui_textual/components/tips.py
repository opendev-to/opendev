"""Tips system for displaying helpful hints below spinners."""

from __future__ import annotations

import random
from typing import List


class TipsManager:
    """Manages rotating tips displayed during processing."""

    TIPS: List[str] = [
        "Create custom slash commands by adding .md files to .opendev/commands/",
        "Use @ to mention files and add them to context (e.g., @README.md)",
        "Press Shift+Tab to toggle between NORMAL and PLAN modes",
        "Use ↑↓ arrow keys to navigate through command history",
        "Use /help to see all available commands",
        "Use /models to switch between different AI models",
        "Page Up/Page Down scroll through long conversations",
        "Press Esc to interrupt long-running operations",
        "Use /mode plan for read-only analysis and planning",
        "Use /mode normal for full execution with file writes",
        "ACE playbooks remember long-term goals without storing long chats",
        "MCP servers extend swecli with custom tools and capabilities",
        "Use /mcp list to see all available MCP servers",
        "Session auto-save preserves your work automatically",
        "Use /clear to start a fresh conversation",
        "Approval rules can be customized for different operations",
    ]

    def __init__(self) -> None:
        self._current_tip_index = 0
        self._random_mode = True

    def get_next_tip(self) -> str:
        """Return the next tip."""
        if self._random_mode:
            return random.choice(self.TIPS)
        tip = self.TIPS[self._current_tip_index]
        self._current_tip_index = (self._current_tip_index + 1) % len(self.TIPS)
        return tip

    def format_tip(self, tip: str, color: str = "\033[38;5;240m") -> str:
        """Format a tip with ANSI styling for terminal fallbacks."""
        reset = "\033[0m"
        return f"{color}  ⎿  Tip: {tip}{reset}"
