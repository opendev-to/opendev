"""Test approval prompt controller interrupt message behavior."""

from unittest.mock import Mock, MagicMock, patch
from rich.text import Text

from swecli.ui_textual.controllers.approval_prompt_controller import ApprovalPromptController


def test_approval_cancel_shows_interrupt_message_with_call_start():
    """Test that cancelling approval (selecting 'No') shows interrupt message when call_start exists."""
    # Setup mocks
    app = Mock()
    conversation = Mock()
    app.conversation = conversation
    app.input_field = Mock()

    # Mock conversation state - tool call in progress
    conversation._tool_display = "some_tool"
    conversation._tool_call_start = 100  # Non-none call_start
    conversation._tool_spinner_timer = Mock()
    conversation._tool_spinner_timer.stop = Mock()

    # Mock clear_approval_prompt and write methods
    conversation.clear_approval_prompt = Mock()
    conversation.write = Mock()
    conversation._replace_tool_call_line = Mock()
    conversation._tool_display = None
    conversation._tool_call_start = None

    # Create controller and mock future
    controller = ApprovalPromptController(app, interrupt_manager=None)
    controller._active = True
    controller._future = MagicMock()
    controller._future.done = Mock(return_value=False)
    controller._future.set_result = Mock()
    controller._options = [
        {"choice": "1", "label": "Yes", "description": "Run this command now.", "approved": True},
        {"choice": "2", "label": "Yes, and don't ask again", "description": "Auto approve.", "approved": True},
        {"choice": "3", "label": "No", "description": "Cancel.", "approved": False},
    ]
    app.input_field.text = ""

    # Select "No" option (index 2)
    controller._selected_index = 2

    # Call confirm
    controller.confirm()

    # Verify interrupt message was shown
    assert conversation.write.called, "conversation.write should have been called to show interrupt message"

    # Get the actual text that was written
    written_args = [call[0][0] for call in conversation.write.call_args_list]
    interrupt_text = None
    for arg in written_args:
        if isinstance(arg, Text):
            # Check if it contains the interrupt marker
            if "⎿" in str(arg) or "Interrupted" in str(arg):
                interrupt_text = arg
                break

    assert interrupt_text is not None, "Interrupt message with '⎿' and 'Interrupted' should have been written"


def test_approval_cancel_shows_interrupt_message_without_call_start():
    """Test that cancelling approval shows interrupt message even when call_start is None.

    This is the key test case for the bug fix - previously the interrupt message
    was only shown if call_start was not None. Now it should ALWAYS be shown.
    """
    # Setup mocks
    app = Mock()
    conversation = Mock()
    app.conversation = conversation
    app.input_field = Mock()

    # Mock conversation state - NO tool call in progress (call_start is None)
    conversation._tool_display = None
    conversation._tool_call_start = None  # <-- This is the key difference
    conversation._tool_spinner_timer = None

    # Mock clear_approval_prompt and write methods
    conversation.clear_approval_prompt = Mock()
    conversation.write = Mock()

    # Create controller and mock future
    controller = ApprovalPromptController(app, interrupt_manager=None)
    controller._active = True
    controller._future = MagicMock()
    controller._future.done = Mock(return_value=False)
    controller._future.set_result = Mock()
    controller._options = [
        {"choice": "1", "label": "Yes", "description": "Run this command now.", "approved": True},
        {"choice": "2", "label": "Yes, and don't ask again", "description": "Auto approve.", "approved": True},
        {"choice": "3", "label": "No", "description": "Cancel.", "approved": False},
    ]
    app.input_field.text = ""

    # Select "No" option (index 2)
    controller._selected_index = 2

    # Call confirm
    controller.confirm()

    # Verify interrupt message was shown (THIS IS THE BUG FIX)
    assert conversation.write.called, "conversation.write should have been called to show interrupt message"

    # Get the actual text that was written
    written_args = [call[0][0] for call in conversation.write.call_args_list]
    interrupt_text = None
    for arg in written_args:
        if isinstance(arg, Text):
            # Check if it contains the interrupt marker
            if "⎿" in str(arg) or "Interrupted" in str(arg):
                interrupt_text = arg
                break

    assert interrupt_text is not None, (
        "Interrupt message with '⎿' and 'Interrupted' should have been written "
        "EVEN WHEN call_start is None (this was the bug)"
    )


if __name__ == "__main__":
    print("Running test_approval_cancel_shows_interrupt_message_with_call_start...")
    test_approval_cancel_shows_interrupt_message_with_call_start()
    print("PASSED\n")

    print("Running test_approval_cancel_shows_interrupt_message_without_call_start...")
    test_approval_cancel_shows_interrupt_message_without_call_start()
    print("PASSED\n")

    print("All tests passed!")
