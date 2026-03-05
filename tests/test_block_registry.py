"""Tests for the BlockRegistry used in terminal resize handling."""

import pytest
from rich.text import Text

from swecli.ui_textual.widgets.conversation.block_registry import (
    BlockRegistry,
    ContentBlock,
)


class TestContentBlock:
    """Tests for ContentBlock dataclass."""

    def test_content_block_creation(self):
        """Test creating a ContentBlock with required fields."""
        block = ContentBlock(
            block_id="test-123",
            source=Text("Hello, world!"),
            is_wrappable=True,
        )
        assert block.block_id == "test-123"
        assert block.is_wrappable is True
        assert block.is_locked is False
        assert block.start_line == 0
        assert block.line_count == 0

    def test_content_block_with_all_fields(self):
        """Test creating a ContentBlock with all fields."""
        block = ContentBlock(
            block_id="test-456",
            source=Text("Test content"),
            is_wrappable=False,
            is_locked=True,
            start_line=10,
            line_count=5,
        )
        assert block.block_id == "test-456"
        assert block.is_wrappable is False
        assert block.is_locked is True
        assert block.start_line == 10
        assert block.line_count == 5


class TestBlockRegistry:
    """Tests for BlockRegistry class."""

    def test_register_block(self):
        """Test registering a block."""
        registry = BlockRegistry()
        block = ContentBlock(
            block_id="block-1",
            source=Text("First block"),
            is_wrappable=True,
            start_line=0,
            line_count=2,
        )
        registry.register(block)

        assert len(registry) == 1
        assert registry.get_block("block-1") is block

    def test_register_multiple_blocks(self):
        """Test registering multiple blocks."""
        registry = BlockRegistry()

        block1 = ContentBlock(
            block_id="block-1",
            source=Text("First"),
            is_wrappable=True,
            start_line=0,
            line_count=1,
        )
        block2 = ContentBlock(
            block_id="block-2",
            source=Text("Second"),
            is_wrappable=False,
            start_line=1,
            line_count=3,
        )

        registry.register(block1)
        registry.register(block2)

        assert len(registry) == 2
        assert registry.get_block("block-1") is block1
        assert registry.get_block("block-2") is block2

    def test_get_nonexistent_block(self):
        """Test getting a block that doesn't exist returns None."""
        registry = BlockRegistry()
        assert registry.get_block("nonexistent") is None

    def test_lock_block(self):
        """Test locking a block."""
        registry = BlockRegistry()
        block = ContentBlock(
            block_id="block-1",
            source=Text("Lockable"),
            is_wrappable=True,
        )
        registry.register(block)

        assert block.is_locked is False
        registry.lock_block("block-1")
        assert block.is_locked is True

    def test_lock_nonexistent_block(self):
        """Test locking a nonexistent block doesn't raise error."""
        registry = BlockRegistry()
        # Should not raise
        registry.lock_block("nonexistent")

    def test_unlock_block(self):
        """Test unlocking a block."""
        registry = BlockRegistry()
        block = ContentBlock(
            block_id="block-1",
            source=Text("Unlockable"),
            is_wrappable=True,
            is_locked=True,
        )
        registry.register(block)

        assert block.is_locked is True
        registry.unlock_block("block-1")
        assert block.is_locked is False

    def test_adjust_indices_after(self):
        """Test adjusting indices for blocks after a given block."""
        registry = BlockRegistry()

        block1 = ContentBlock(
            block_id="block-1",
            source=Text("First"),
            is_wrappable=True,
            start_line=0,
            line_count=2,
        )
        block2 = ContentBlock(
            block_id="block-2",
            source=Text("Second"),
            is_wrappable=True,
            start_line=2,
            line_count=3,
        )
        block3 = ContentBlock(
            block_id="block-3",
            source=Text("Third"),
            is_wrappable=True,
            start_line=5,
            line_count=1,
        )

        registry.register(block1)
        registry.register(block2)
        registry.register(block3)

        # Simulate block1 gaining 2 extra lines
        registry.adjust_indices_after(block1, 2)

        # block1 should be unchanged
        assert block1.start_line == 0
        # block2 and block3 should be shifted
        assert block2.start_line == 4  # 2 + 2
        assert block3.start_line == 7  # 5 + 2

    def test_adjust_indices_negative_delta(self):
        """Test adjusting indices with negative delta (content shrunk)."""
        registry = BlockRegistry()

        block1 = ContentBlock(
            block_id="block-1",
            source=Text("First"),
            is_wrappable=True,
            start_line=0,
            line_count=5,
        )
        block2 = ContentBlock(
            block_id="block-2",
            source=Text("Second"),
            is_wrappable=True,
            start_line=5,
            line_count=3,
        )

        registry.register(block1)
        registry.register(block2)

        # Simulate block1 losing 2 lines
        registry.adjust_indices_after(block1, -2)

        assert block1.start_line == 0
        assert block2.start_line == 3  # 5 - 2

    def test_remove_blocks_from(self):
        """Test removing blocks from a given start line."""
        registry = BlockRegistry()

        block1 = ContentBlock(
            block_id="block-1",
            source=Text("First"),
            is_wrappable=True,
            start_line=0,
            line_count=2,
        )
        block2 = ContentBlock(
            block_id="block-2",
            source=Text("Second"),
            is_wrappable=True,
            start_line=2,
            line_count=3,
        )
        block3 = ContentBlock(
            block_id="block-3",
            source=Text("Third"),
            is_wrappable=True,
            start_line=5,
            line_count=1,
        )

        registry.register(block1)
        registry.register(block2)
        registry.register(block3)

        # Remove blocks starting at line 2 or later
        registry.remove_blocks_from(2)

        assert len(registry) == 1
        assert registry.get_block("block-1") is block1
        assert registry.get_block("block-2") is None
        assert registry.get_block("block-3") is None

    def test_remove_blocks_from_removes_exact_match(self):
        """Test that remove_blocks_from includes the exact start_line."""
        registry = BlockRegistry()

        block = ContentBlock(
            block_id="block-1",
            source=Text("At boundary"),
            is_wrappable=True,
            start_line=5,
            line_count=2,
        )
        registry.register(block)

        registry.remove_blocks_from(5)

        assert len(registry) == 0

    def test_get_wrappable_unlocked_blocks(self):
        """Test getting wrappable and unlocked blocks."""
        registry = BlockRegistry()

        # Wrappable and unlocked
        block1 = ContentBlock(
            block_id="block-1",
            source=Text("Wrappable unlocked"),
            is_wrappable=True,
            is_locked=False,
        )
        # Wrappable but locked
        block2 = ContentBlock(
            block_id="block-2",
            source=Text("Wrappable locked"),
            is_wrappable=True,
            is_locked=True,
        )
        # Not wrappable
        block3 = ContentBlock(
            block_id="block-3",
            source=Text("Non-wrappable"),
            is_wrappable=False,
            is_locked=False,
        )
        # Not wrappable and locked
        block4 = ContentBlock(
            block_id="block-4",
            source=Text("Non-wrappable locked"),
            is_wrappable=False,
            is_locked=True,
        )

        registry.register(block1)
        registry.register(block2)
        registry.register(block3)
        registry.register(block4)

        wrappable = registry.get_wrappable_unlocked_blocks()

        assert len(wrappable) == 1
        assert block1 in wrappable
        assert block2 not in wrappable
        assert block3 not in wrappable
        assert block4 not in wrappable

    def test_get_all_blocks(self):
        """Test getting all blocks in order."""
        registry = BlockRegistry()

        block1 = ContentBlock(
            block_id="block-1",
            source=Text("First"),
            is_wrappable=True,
        )
        block2 = ContentBlock(
            block_id="block-2",
            source=Text("Second"),
            is_wrappable=False,
        )

        registry.register(block1)
        registry.register(block2)

        all_blocks = registry.get_all_blocks()

        assert len(all_blocks) == 2
        assert all_blocks[0] is block1
        assert all_blocks[1] is block2

    def test_get_all_blocks_returns_copy(self):
        """Test that get_all_blocks returns a copy, not the internal list."""
        registry = BlockRegistry()

        block = ContentBlock(
            block_id="block-1",
            source=Text("Test"),
            is_wrappable=True,
        )
        registry.register(block)

        all_blocks = registry.get_all_blocks()
        all_blocks.clear()

        # Internal list should not be affected
        assert len(registry) == 1

    def test_clear(self):
        """Test clearing all blocks."""
        registry = BlockRegistry()

        registry.register(ContentBlock(
            block_id="block-1",
            source=Text("One"),
            is_wrappable=True,
        ))
        registry.register(ContentBlock(
            block_id="block-2",
            source=Text("Two"),
            is_wrappable=True,
        ))

        assert len(registry) == 2

        registry.clear()

        assert len(registry) == 0
        assert registry.get_block("block-1") is None
        assert registry.get_block("block-2") is None

    def test_len(self):
        """Test __len__ method."""
        registry = BlockRegistry()

        assert len(registry) == 0

        registry.register(ContentBlock(
            block_id="block-1",
            source=Text("One"),
            is_wrappable=True,
        ))
        assert len(registry) == 1

        registry.register(ContentBlock(
            block_id="block-2",
            source=Text("Two"),
            is_wrappable=True,
        ))
        assert len(registry) == 2
