"""Test that ConversationLog correctly renders interrupted messages."""
from rich.text import Text
from swecli.ui_textual.widgets.conversation_log import ConversationLog


def test_interrupted_message_rendering():
    """Test that ::interrupted:: marker is handled correctly."""
    # Create a conversation log
    log = ConversationLog()

    # Simulate adding an interrupted tool result
    result_text = "  ⎿ ::interrupted:: Interrupted · What should I do instead?"

    # Add the result (this calls _write_generic_tool_result internally)
    log.add_tool_result(result_text)

    # Get the rendered lines
    lines = log.lines

    print("\n" + "="*60)
    print(f"Total lines rendered: {len(lines)}")
    print("="*60)

    for i, line in enumerate(lines):
        if hasattr(line, 'plain'):
            plain = line.plain
            print(f"Line {i}: {repr(plain)}")
            # Check if this is the interrupted message
            if "Interrupted" in plain and "should I do" in plain:
                print(f"  -> Found interrupted message")
                print(f"  -> Style: {line.spans if hasattr(line, 'spans') else 'N/A'}")

                # Verify it doesn't have the ❌ icon
                assert "❌" not in plain, "Should not contain ❌ icon"
                assert "::interrupted::" not in plain, "Should have marker stripped"
                assert "Interrupted" in plain, "Should contain 'Interrupted'"
                assert "What should I do instead?" in plain, "Should contain message"
                print(f"  -> ✅ Correct: No ❌ icon, marker stripped")
        else:
            print(f"Line {i}: {line}")

    print("="*60)
    print("✅ Test passed! ConversationLog correctly handles ::interrupted:: marker")


if __name__ == "__main__":
    test_interrupted_message_rendering()
