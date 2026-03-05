"""Block-based content tracking for resize re-rendering."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from rich.console import RenderableType


@dataclass
class ContentBlock:
    """Tracks a content block for resize re-rendering.

    Attributes:
        block_id: Unique identifier (uuid)
        source: Original renderable for re-rendering
        is_wrappable: Whether content should reflow on resize
        is_locked: Whether block is locked during animation (prevents re-render)
        start_line: First line index in self.lines
        line_count: Number of lines this block occupies
    """

    block_id: str
    source: RenderableType
    is_wrappable: bool
    is_locked: bool = False
    start_line: int = 0
    line_count: int = 0


class BlockRegistry:
    """Registry for tracking content blocks for resize re-rendering.

    This registry maintains a mapping between content blocks and their
    line positions in the RichLog. When the terminal is resized, wrappable
    and unlocked blocks can be re-rendered at the new width.
    """

    def __init__(self) -> None:
        self._blocks: list[ContentBlock] = []
        self._block_map: dict[str, ContentBlock] = {}  # block_id -> block

    def register(self, block: ContentBlock) -> None:
        """Add a new block at the end.

        Args:
            block: ContentBlock to register
        """
        self._blocks.append(block)
        self._block_map[block.block_id] = block

    def get_block(self, block_id: str) -> Optional[ContentBlock]:
        """Get a block by its ID.

        Args:
            block_id: Unique identifier of the block

        Returns:
            ContentBlock if found, None otherwise
        """
        return self._block_map.get(block_id)

    def lock_block(self, block_id: str) -> None:
        """Lock a block to prevent re-rendering during animation.

        Args:
            block_id: Unique identifier of the block to lock
        """
        if block := self._block_map.get(block_id):
            block.is_locked = True

    def unlock_block(self, block_id: str) -> None:
        """Unlock a block to allow re-rendering.

        Args:
            block_id: Unique identifier of the block to unlock
        """
        if block := self._block_map.get(block_id):
            block.is_locked = False

    def adjust_indices_after(self, after_block: ContentBlock, delta: int) -> None:
        """Shift start_line for all blocks after the given block.

        Used when a block's line count changes during re-rendering.

        Args:
            after_block: The block after which to adjust indices
            delta: Amount to shift (positive = added lines, negative = removed)
        """
        found = False
        for block in self._blocks:
            if found:
                block.start_line += delta
            if block.block_id == after_block.block_id:
                found = True

    def remove_blocks_from(self, start_line: int) -> None:
        """Remove blocks that start at or after start_line.

        Used when content is truncated (e.g., clearing approval prompts).

        Args:
            start_line: Line index from which to remove blocks
        """
        self._blocks = [b for b in self._blocks if b.start_line < start_line]
        self._block_map = {b.block_id: b for b in self._blocks}

    def remove_lines_range(self, start: int, count: int) -> None:
        """Remove blocks affected by line deletion and adjust remaining indices.

        When lines are deleted from the RichLog, this method keeps the registry
        in sync by removing blocks that occupied the deleted range and shifting
        subsequent blocks backward.

        Args:
            start: First deleted line index
            count: Number of lines deleted
        """
        end = start + count
        new_blocks = []
        for b in self._blocks:
            if start <= b.start_line < end:
                continue  # Remove blocks in deleted range
            if b.start_line >= end:
                b.start_line -= count  # Shift blocks after deleted range
            new_blocks.append(b)
        self._blocks = new_blocks
        self._block_map = {b.block_id: b for b in self._blocks}

    def get_wrappable_unlocked_blocks(self) -> list[ContentBlock]:
        """Get blocks that should be re-rendered on resize.

        Returns:
            List of ContentBlocks that are wrappable and not locked
        """
        return [b for b in self._blocks if b.is_wrappable and not b.is_locked]

    def get_all_blocks(self) -> list[ContentBlock]:
        """Get all registered blocks in order.

        Returns:
            List of all ContentBlocks
        """
        return self._blocks.copy()

    def clear(self) -> None:
        """Clear all registered blocks."""
        self._blocks.clear()
        self._block_map.clear()

    def __len__(self) -> int:
        """Return the number of registered blocks."""
        return len(self._blocks)
