"""Handler for self-critique of reasoning/thinking content."""

from dataclasses import dataclass, field
from datetime import datetime
from typing import List, Optional


@dataclass
class CritiqueBlock:
    """A single block of critique content."""

    id: str
    content: str
    original_thinking: str
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())


class CritiqueHandler:
    """Handler for self-critique - analyzes and critiques thinking traces.

    This handler manages critique content produced when self-critique mode
    is enabled. After a thinking trace is generated, the critique phase
    analyzes the reasoning and provides feedback to improve it.
    """

    def __init__(self):
        """Initialize critique handler with empty state."""
        self._blocks: List[CritiqueBlock] = []
        self._next_id = 1
        self._visible = False  # Default OFF - self-critique disabled by default

    def add_critique(self, critique: str, original_thinking: str) -> dict:
        """Add a critique block.

        Args:
            critique: The critique/feedback content
            original_thinking: The thinking trace that was critiqued

        Returns:
            Result dict with success status and special keys for UI
        """
        if not critique or not critique.strip():
            return {
                "success": False,
                "error": "Critique content cannot be empty",
                "output": "",
            }

        block_id = f"critique-{self._next_id}"
        self._next_id += 1

        block = CritiqueBlock(
            id=block_id,
            content=critique.strip(),
            original_thinking=original_thinking,
        )
        self._blocks.append(block)

        return {
            "success": True,
            "output": critique.strip(),
            "critique_id": block_id,
            "_critique_content": critique.strip(),
        }

    def get_all_critiques(self) -> List[CritiqueBlock]:
        """Get all critique blocks for current turn.

        Returns:
            List of CritiqueBlock objects
        """
        return list(self._blocks)

    def get_latest_critique(self) -> Optional[CritiqueBlock]:
        """Get the most recent critique block.

        Returns:
            The latest CritiqueBlock or None if empty
        """
        return self._blocks[-1] if self._blocks else None

    def clear(self) -> None:
        """Clear all critique blocks.

        Should be called when a new user message is processed
        to reset the critique state for the new turn.
        """
        self._blocks.clear()
        self._next_id = 1

    def toggle_visibility(self) -> bool:
        """Toggle global visibility/enabled state of self-critique.

        Returns:
            New visibility state (True = enabled)
        """
        self._visible = not self._visible
        return self._visible

    def set_visible(self, visible: bool) -> None:
        """Set the visibility/enabled state directly.

        Args:
            visible: Whether self-critique should be enabled
        """
        self._visible = visible

    @property
    def is_visible(self) -> bool:
        """Check if self-critique is enabled.

        Returns:
            True if self-critique is enabled
        """
        return self._visible

    @property
    def block_count(self) -> int:
        """Get the number of critique blocks.

        Returns:
            Number of critique blocks stored
        """
        return len(self._blocks)
