"""Test interrupt display via UI callback."""
from unittest.mock import Mock
from rich.text import Text
from swecli.ui_textual.ui_callback import TextualUICallback


def test_on_interrupt_removes_blank_line_and_shows_message():
    """Test that on_interrupt removes trailing blank line and displays interrupt message."""

    # Create mock conversation widget with lines array
    mock_conversation = Mock()
    mock_app = Mock()

    # Mock lines array with a blank line (simulating what add_user_message adds)
    mock_conversation.lines = [
        Text("› run @app.py", style="bold white"),
        Text(""),  # Blank line added by add_user_message
    ]

    # Track calls to write (the actual method used to display interrupt)
    write_calls = []
    def track_write(text):
        write_calls.append(text)

    mock_conversation.write = track_write
    mock_conversation.stop_spinner = Mock()

    # Track calls to _truncate_from
    truncate_calls = []
    def track_truncate(index):
        truncate_calls.append(index)

    mock_conversation._truncate_from = track_truncate

    # Create UI callback
    ui_callback = TextualUICallback(mock_conversation, mock_app)
    # _run_on_ui checks if self._app is not None, so it should be set
    # But in our test, we want direct execution, so we can mock call_from_thread
    ui_callback._app = None  # This will make _run_on_ui call func() directly

    # Call on_interrupt
    ui_callback.on_interrupt()

    # Verify the blank line was removed
    print(f"\n=== Test Results ===")
    print(f"Number of calls to _truncate_from: {len(truncate_calls)}")
    if truncate_calls:
        print(f"Truncated at index: {truncate_calls[0]}")
        assert len(truncate_calls) == 1, f"Expected 1 truncate call, got {len(truncate_calls)}"
        assert truncate_calls[0] == 1, f"Expected truncate at index 1 (to remove blank line), got {truncate_calls[0]}"
        print("✅ Blank line removed correctly")
    else:
        print("❌ Test FAILED - _truncate_from was not called")
        raise AssertionError("on_interrupt did not remove blank line")

    # Verify write was called with interrupt message
    print(f"Number of calls to write: {len(write_calls)}")

    if write_calls:
        call_text = write_calls[0]
        print(f"Called with: {call_text}")

        # Assertions - the write call receives a Rich Text object
        assert len(write_calls) == 1, f"Expected 1 call, got {len(write_calls)}"

        # Convert to plain text for checking content
        if hasattr(call_text, "plain"):
            plain_text = call_text.plain
        else:
            plain_text = str(call_text)

        assert "Interrupted" in plain_text, "Expected 'Interrupted' in message"
        assert "What should I do instead?" in plain_text, "Expected 'What should I do instead?'"

        print("✅ Test PASSED - on_interrupt displays correct message")
    else:
        print("❌ Test FAILED - write was not called")
        raise AssertionError("on_interrupt did not call write")


def test_on_interrupt_without_blank_line():
    """Test that on_interrupt works even when there's no trailing blank line."""

    # Create mock conversation widget with no blank line
    mock_conversation = Mock()
    mock_app = Mock()

    # Mock lines array WITHOUT a blank line at the end
    mock_conversation.lines = [
        Text("› run @app.py", style="bold white"),
    ]

    # Track calls to write
    write_calls = []
    mock_conversation.write = lambda text: write_calls.append(text)
    mock_conversation.stop_spinner = Mock()
    mock_conversation._truncate_from = Mock()

    # Create UI callback
    ui_callback = TextualUICallback(mock_conversation, mock_app)
    ui_callback._app = None

    # Call on_interrupt
    ui_callback.on_interrupt()

    # Should NOT call _truncate_from since last line is not blank
    assert mock_conversation._truncate_from.call_count == 0, \
        "Should not truncate when last line is not blank"

    # Should still display interrupt message
    assert len(write_calls) == 1, "Should still display interrupt message"


if __name__ == "__main__":
    test_on_interrupt_removes_blank_line_and_shows_message()
    test_on_interrupt_without_blank_line()
