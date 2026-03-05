"""Handler for capturing model reasoning/thinking content."""

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import List, Optional


class ThinkingLevel(Enum):
    """Thinking mode levels controlling reasoning depth and self-critique.

    - OFF: No thinking phase
    - LOW: Brief thinking (concise, ~50 words)
    - MEDIUM: Standard thinking (balanced, ~100 words) - default
    - HIGH: Detailed thinking with self-critique (thorough, ~200 words)
    """
    OFF = "Off"
    LOW = "Low"
    MEDIUM = "Medium"
    HIGH = "High"

    def next(self) -> "ThinkingLevel":
        """Get the next level in the cycle."""
        levels = list(ThinkingLevel)
        idx = levels.index(self)
        return levels[(idx + 1) % len(levels)]

    @property
    def is_enabled(self) -> bool:
        """Check if thinking is enabled at this level."""
        return self != ThinkingLevel.OFF

    @property
    def includes_critique(self) -> bool:
        """Check if this level includes self-critique."""
        return self == ThinkingLevel.HIGH

    @property
    def word_limit(self) -> int:
        """Get the suggested word limit for this level."""
        limits = {
            ThinkingLevel.OFF: 0,
            ThinkingLevel.LOW: 50,
            ThinkingLevel.MEDIUM: 100,
            ThinkingLevel.HIGH: 200,
        }
        return limits.get(self, 100)


@dataclass
class ThinkingBlock:
    """A single block of thinking content."""

    id: str
    content: str
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())


class ThinkingHandler:
    """Handler for think tool - captures model reasoning.

    This handler manages thinking content that the model produces
    when using the 'think' tool to reason through complex problems.
    The content can be displayed in the UI with dark gray styling
    and the thinking level can be cycled via hotkey.
    """

    def __init__(self):
        """Initialize thinking handler with empty state."""
        self._blocks: List[ThinkingBlock] = []
        self._next_id = 1
        self._level = ThinkingLevel.MEDIUM  # Default to medium thinking

    def add_thinking(self, thought: str) -> dict:
        """Add a thinking block.

        Args:
            thought: The reasoning/thinking content from the model

        Returns:
            Result dict with success status and special keys for UI
        """
        if not thought or not thought.strip():
            return {
                "success": False,
                "error": "Thinking content cannot be empty",
                "output": "",  # Empty string, not None (APIs require string)
            }

        block_id = f"think-{self._next_id}"
        self._next_id += 1

        block = ThinkingBlock(id=block_id, content=thought.strip())
        self._blocks.append(block)

        return {
            "success": True,
            "output": thought.strip(),  # Include in message history for subsequent LLM calls
            "thinking_id": block_id,
            "_thinking_content": thought.strip(),  # Special key for UI callback
        }

    def get_all_thinking(self) -> List[ThinkingBlock]:
        """Get all thinking blocks for current turn.

        Returns:
            List of ThinkingBlock objects
        """
        return list(self._blocks)

    def get_latest_thinking(self) -> Optional[ThinkingBlock]:
        """Get the most recent thinking block.

        Returns:
            The latest ThinkingBlock or None if empty
        """
        return self._blocks[-1] if self._blocks else None

    def clear(self) -> None:
        """Clear all thinking blocks.

        Should be called when a new user message is processed
        to reset the thinking state for the new turn.
        """
        self._blocks.clear()
        self._next_id = 1

    def cycle_level(self) -> ThinkingLevel:
        """Cycle to the next thinking level.

        Returns:
            New thinking level
        """
        self._level = self._level.next()
        return self._level

    def set_level(self, level: ThinkingLevel) -> None:
        """Set the thinking level directly.

        Args:
            level: The thinking level to set
        """
        self._level = level

    @property
    def level(self) -> ThinkingLevel:
        """Get the current thinking level.

        Returns:
            Current ThinkingLevel
        """
        return self._level

    @property
    def is_visible(self) -> bool:
        """Check if thinking content should be displayed.

        Returns:
            True if thinking is enabled (level != OFF)
        """
        return self._level.is_enabled

    @property
    def includes_critique(self) -> bool:
        """Check if current level includes self-critique.

        Returns:
            True if level is HIGH
        """
        return self._level.includes_critique

    # Legacy compatibility - maps to is_visible
    @property
    def _visible(self) -> bool:
        return self.is_visible

    @_visible.setter
    def _visible(self, value: bool) -> None:
        # Legacy setter - if setting to False, set OFF; if True, set MEDIUM
        if value:
            if self._level == ThinkingLevel.OFF:
                self._level = ThinkingLevel.MEDIUM
        else:
            self._level = ThinkingLevel.OFF

    def toggle_visibility(self) -> bool:
        """Legacy method - cycles through levels.

        Returns:
            New visibility state (True = enabled)
        """
        self.cycle_level()
        return self.is_visible

    @property
    def block_count(self) -> int:
        """Get the number of thinking blocks.

        Returns:
            Number of thinking blocks stored
        """
        return len(self._blocks)
