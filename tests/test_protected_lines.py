"""Tests for protected line tracking system in ConversationLog."""

import pytest
from rich.text import Text
from swecli.ui_textual.widgets.conversation_log import ConversationLog


class TestProtectedLineTracking:
    """Test suite for protected line tracking and debug message persistence."""

    @pytest.fixture
    def conversation_log(self):
        """Create a ConversationLog instance for testing."""
        log = ConversationLog()
        # Initialize lines list for testing (RichLog normally populates this when mounted)
        log.lines = []
        return log

    def test_debug_enabled_flag(self, conversation_log):
        """Test set_debug_enabled flag."""
        assert conversation_log._debug_enabled is False
        conversation_log.set_debug_enabled(True)
        assert conversation_log._debug_enabled is True
        conversation_log.set_debug_enabled(False)
        assert conversation_log._debug_enabled is False

    def test_truncate_from_preserves_protected_lines(self, conversation_log):
        """Test that _truncate_from() preserves protected lines."""
        # Add 5 lines
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))

        # Mark lines 1 and 3 as protected
        conversation_log._protected_lines.add(1)
        conversation_log._protected_lines.add(3)

        # Truncate from line 1 (should delete lines 1-4 but preserve protected ones)
        conversation_log._truncate_from(1)

        # Should have 3 lines now: Line 0, and the two protected lines
        assert len(conversation_log.lines) == 3

        # Protected lines should be at indices 1 and 2 now
        assert 1 in conversation_log._protected_lines
        assert 2 in conversation_log._protected_lines
        assert 3 not in conversation_log._protected_lines  # Old index removed

    def test_truncate_from_no_protected_lines(self, conversation_log):
        """Test _truncate_from() without protected lines (original behavior)."""
        # Add 5 lines
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))

        # Truncate from line 2
        conversation_log._truncate_from(2)

        # Should have 2 lines now
        assert len(conversation_log.lines) == 2

    def test_multiple_protected_lines_preserved(self, conversation_log):
        """Test that multiple protected lines are all preserved."""
        # Add 10 lines, mark even indices as protected
        for i in range(10):
            conversation_log.lines.append(Text(f"Line {i}"))
            if i % 2 == 0:
                conversation_log._protected_lines.add(i)

        # Truncate from line 3
        conversation_log._truncate_from(3)

        # Should have: Line 0, 1, 2, and protected lines 4, 6, 8
        # Total = 3 + 3 = 6 lines
        assert len(conversation_log.lines) == 6
        # Protected: 0, 2 (kept before truncation point) + 4, 6, 8 (re-added) = 5 protected lines
        assert len(conversation_log._protected_lines) == 5

    def test_remove_spinner_lines_preserves_protected(self, conversation_log):
        """Test that _remove_spinner_lines() preserves protected lines."""
        # Add 5 lines
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))

        # Mark line 3 as protected
        conversation_log._protected_lines.add(3)

        # Set spinner start at line 2
        conversation_log._spinner_manager._spinner_start = 2
        conversation_log._spinner_manager._spinner_line_count = 3  # Assume 3 lines

        # Remove spinner lines
        conversation_log._spinner_manager._remove_spinner_lines()

        # Should have 3 lines: 0, 1, and the protected line (originally 3)
        assert len(conversation_log.lines) == 3
        assert 2 in conversation_log._protected_lines  # Moved from index 3 to 2

    def test_prune_old_protected_lines(self, conversation_log):
        """Test that old protected lines are pruned when exceeding max."""
        conversation_log.MAX_PROTECTED_LINES = 5  # Lower limit for testing

        # Add 10 lines, all protected
        for i in range(10):
            conversation_log.lines.append(Text(f"Line {i}"))
            conversation_log._protected_lines.add(i)

        # Manually trigger pruning
        conversation_log._prune_old_protected_lines()

        # Should only keep the 5 most recent
        assert len(conversation_log._protected_lines) == 5

        # The kept indices should be 5-9
        expected = {5, 6, 7, 8, 9}
        assert conversation_log._protected_lines == expected

    def test_cleanup_protected_lines_removes_out_of_bounds(self, conversation_log):
        """Test that cleanup removes out-of-bounds indices."""
        # Add 5 lines
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))

        # Mark lines 1, 2, 3 as protected, plus add some out-of-bounds indices
        conversation_log._protected_lines = {1, 2, 3, 10, 100, 500}

        # Cleanup should remove out-of-bounds indices
        conversation_log._cleanup_protected_lines()

        assert conversation_log._protected_lines == {1, 2, 3}

    def test_protected_lines_bubble_up_complex(self, conversation_log):
        """Test complex scenario with multiple truncations."""
        # Add 10 lines
        for i in range(10):
            conversation_log.lines.append(Text(f"Line {i}"))

        # Mark lines 2, 5, 8 as protected
        conversation_log._protected_lines = {2, 5, 8}

        # First truncation from line 4
        conversation_log._truncate_from(4)

        # Should have: 0, 1, 2, 3, and protected lines 5, 8
        # = 6 lines total
        assert len(conversation_log.lines) == 6
        assert len(conversation_log._protected_lines) == 3

        # Second truncation from line 2
        conversation_log._truncate_from(2)

        # Should have: 0, 1, and protected lines (2, 5->4, 8->5)
        assert len(conversation_log.lines) == 5
        assert len(conversation_log._protected_lines) == 3

    def test_empty_protected_lines(self, conversation_log):
        """Test truncation with no protected lines."""
        # Add 5 lines
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))

        # No protected lines
        assert len(conversation_log._protected_lines) == 0

        # Truncate
        conversation_log._truncate_from(2)

        assert len(conversation_log.lines) == 2
        assert len(conversation_log._protected_lines) == 0

    def test_all_lines_protected(self, conversation_log):
        """Test when all lines are protected."""
        # Add 5 lines, all protected
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))
            conversation_log._protected_lines.add(i)

        # Truncate from line 2
        conversation_log._truncate_from(2)

        # Should have all 5 lines (preserved)
        assert len(conversation_log.lines) == 5
        assert len(conversation_log._protected_lines) == 5


class TestDebugEnabledCheck:
    """Test debug enabled/disabled behavior."""

    def test_debug_messages_disabled_by_default(self):
        """Test that debug is disabled by default."""
        log = ConversationLog()
        assert log._debug_enabled is False

    def test_debug_messages_can_be_enabled(self):
        """Test that debug can be enabled."""
        log = ConversationLog()
        log.set_debug_enabled(True)
        assert log._debug_enabled is True


class TestApprovalPanelWithProtectedLines:
    """Test approval panel interactions with protected line tracking."""

    @pytest.fixture
    def conversation_log(self):
        """Create a ConversationLog instance for testing."""
        log = ConversationLog()
        log.lines = []
        return log

    def test_approval_start_updated_after_truncation(self, conversation_log):
        """Test that _approval_start is updated correctly after truncation with protected lines."""
        # Add some regular lines
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))

        # Add protected debug messages at lines 3 and 4
        conversation_log._protected_lines.add(3)
        conversation_log._protected_lines.add(4)

        # Set approval start to line 5 (simulating first render)
        conversation_log._approval_start = 5

        # Call truncate_from as render_approval_prompt does
        conversation_log._truncate_from(conversation_log._approval_start)

        # After truncation, protected lines should be re-added
        # So we should have lines 0-4 (with 3 and 4 being the protected ones)
        assert len(conversation_log.lines) == 5

        # Now if we update _approval_start as the fix does:
        conversation_log._approval_start = len(conversation_log.lines)

        # It should now point to position 5 (where new approval lines would be added)
        assert conversation_log._approval_start == 5

    def test_clear_approval_preserves_protected_lines(self, conversation_log):
        """Test that clearing approval panel preserves protected lines."""
        # Add lines and protected lines
        for i in range(5):
            conversation_log.lines.append(Text(f"Line {i}"))
        conversation_log._protected_lines.add(3)
        conversation_log._protected_lines.add(4)

        # Render approval panel
        approval_lines = [Text("Approval Panel Line 1")]
        conversation_log.render_approval_prompt(approval_lines)

        # Clear approval panel
        conversation_log.clear_approval_prompt()

        # Should have original lines plus protected ones
        # Lines 0-2 remain, lines 3-4 are protected and should still be there
        assert len(conversation_log.lines) == 5
        assert 3 in conversation_log._protected_lines
        assert 4 in conversation_log._protected_lines

    def test_multiple_approval_renders_with_protected_lines(self, conversation_log):
        """Test multiple approval panel renders with protected lines."""
        # Setup initial state
        for i in range(3):
            conversation_log.lines.append(Text(f"Line {i}"))
        conversation_log._protected_lines.add(2)

        # First approval render
        conversation_log.render_approval_prompt([Text("Approval 1")])
        first_start = conversation_log._approval_start

        # Second approval render (should replace first)
        conversation_log.render_approval_prompt([Text("Approval 2"), Text("Approval 2b")])

        # Protected line should still exist
        assert 2 in conversation_log._protected_lines

        # Approval panel should be correctly positioned
        assert conversation_log._approval_start >= 3  # After protected line


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
