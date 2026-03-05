"""Test interrupt display formatting end-to-end."""
from swecli.ui_textual.formatters.style_formatter import StyleFormatter


def test_interrupt_display_format():
    """Test that interrupted tool results are formatted correctly."""
    formatter = StyleFormatter()

    # Simulate an interrupted bash command result
    tool_name = "run_command"
    tool_args = {"command": "sleep 30"}
    result = {
        "success": False,
        "error": "Command interrupted by user",
        "output": None,
        "exit_code": -1,
    }

    # Format the result
    formatted = formatter.format_tool_result(tool_name, tool_args, result)

    print("\n" + "="*60)
    print("FORMATTED OUTPUT:")
    print("="*60)
    print(formatted)
    print("="*60)

    # Verify the formatted output
    lines = formatted.split("\n")

    # First line should be the tool call with ⏺
    assert lines[0].startswith("⏺"), f"Expected first line to start with ⏺, got: {lines[0]}"
    assert "Bash" in lines[0] or "command" in lines[0], f"Expected 'Bash' or 'command' in first line, got: {lines[0]}"

    # Second line should be the interrupted message with ::interrupted:: marker
    assert len(lines) >= 2, f"Expected at least 2 lines, got {len(lines)}"
    result_line = lines[1].strip()

    # Should contain the ::interrupted:: marker
    assert "::interrupted::" in result_line, f"Expected ::interrupted:: marker, got: {result_line}"

    # Should contain the expected message
    assert "Interrupted" in result_line, f"Expected 'Interrupted', got: {result_line}"
    assert "What should I do instead?" in result_line, f"Expected 'What should I do instead?', got: {result_line}"

    # Should NOT contain ::tool_error:: marker (which would show ❌)
    assert "::tool_error::" not in result_line, f"Should not contain ::tool_error::, got: {result_line}"

    print("\n✅ Test passed! Display format is correct:")
    print(f"   - Tool call line: {lines[0]}")
    print(f"   - Result line: {result_line}")
    print(f"   - Uses ::interrupted:: marker (will show in red without ❌)")


if __name__ == "__main__":
    test_interrupt_display_format()
