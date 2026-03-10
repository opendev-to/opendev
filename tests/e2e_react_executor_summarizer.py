"""E2E test for thinking phase integration with ReactExecutor.

Tests that _get_thinking_trace() passes the full conversation history
with a swapped system prompt and appended analysis prompt.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from unittest.mock import MagicMock
from opendev.repl.react_executor import ReactExecutor


def test_thinking_trace_passes_full_history():
    """Test that _get_thinking_trace passes full message history with swapped system prompt."""
    print("\n" + "=" * 60)
    print("E2E TEST: _get_thinking_trace Full History")
    print("=" * 60)

    executor = ReactExecutor(
        session_manager=MagicMock(),
        config=MagicMock(),
        mode_manager=MagicMock(),
        console=MagicMock(),
        llm_caller=MagicMock(),
        tool_executor=MagicMock(),
    )

    # Create messages with system prompt and conversation history
    messages = [
        {"role": "system", "content": "Original system prompt"},
        {"role": "user", "content": "Question 1"},
        {"role": "assistant", "content": "Answer 1"},
        {"role": "user", "content": "Question 2"},
        {"role": "assistant", "content": "Answer 2"},
        {"role": "user", "content": "Question 3"},
        {"role": "assistant", "content": "Answer 3"},
    ]

    # Track what gets passed to call_thinking_llm
    thinking_llm_calls = []

    def capture_thinking_call(msgs, monitor=None):
        thinking_llm_calls.append(msgs)
        return {"success": True, "content": "I should analyze the user's question..."}

    mock_agent = MagicMock()
    mock_agent.call_thinking_llm = capture_thinking_call
    mock_agent.build_system_prompt = MagicMock(return_value="Thinking system prompt")

    # Call _get_thinking_trace
    result = executor._get_thinking_trace(messages, mock_agent, ui_callback=None)

    # Verify the thinking LLM received the full history
    assert len(thinking_llm_calls) == 1, "Thinking LLM should be called once"
    thinking_msgs = thinking_llm_calls[0]

    # System prompt should be swapped to thinking prompt
    assert thinking_msgs[0]["role"] == "system"
    assert thinking_msgs[0]["content"] == "Thinking system prompt"
    print("✓ System prompt swapped to thinking prompt")

    # All conversation history should be present (system + 6 messages + analysis prompt)
    assert len(thinking_msgs) == 8, f"Expected 8 messages, got {len(thinking_msgs)}"
    print(f"✓ Full history passed: {len(thinking_msgs)} messages")

    # User/assistant pairs should be preserved
    assert thinking_msgs[1]["content"] == "Question 1"
    assert thinking_msgs[2]["content"] == "Answer 1"
    assert thinking_msgs[5]["content"] == "Question 3"
    assert thinking_msgs[6]["content"] == "Answer 3"
    print("✓ Conversation pairs preserved in order")

    # Last message should be analysis prompt
    last_msg = thinking_msgs[-1]
    assert last_msg["role"] == "user"
    assert "Analyze the context" in last_msg["content"]
    print("✓ Analysis prompt appended as final user message")

    # Original messages should NOT be mutated
    assert len(messages) == 7, "Original messages should not be modified"
    assert messages[0]["content"] == "Original system prompt"
    print("✓ Original messages not mutated")

    # Return value
    assert result == "I should analyze the user's question..."
    print("✓ Thinking trace returned correctly")

    print("\n" + "=" * 60)
    print("_get_thinking_trace TEST PASSED ✓")
    print("=" * 60)


def test_build_messages_with_system_prompt():
    """Test the shared _build_messages_with_system_prompt helper."""
    print("\n" + "=" * 60)
    print("E2E TEST: _build_messages_with_system_prompt")
    print("=" * 60)

    executor = ReactExecutor(
        session_manager=MagicMock(),
        config=MagicMock(),
        mode_manager=MagicMock(),
        console=MagicMock(),
        llm_caller=MagicMock(),
        tool_executor=MagicMock(),
    )

    # Test with existing system message
    messages = [
        {"role": "system", "content": "Original prompt"},
        {"role": "user", "content": "Hello"},
    ]
    result = executor._build_messages_with_system_prompt(messages, "New prompt")
    assert result[0]["content"] == "New prompt"
    assert len(result) == 2
    assert messages[0]["content"] == "Original prompt"  # Not mutated
    print("✓ Replaces existing system prompt")

    # Test without system message
    messages_no_system = [
        {"role": "user", "content": "Hello"},
    ]
    result = executor._build_messages_with_system_prompt(messages_no_system, "New prompt")
    assert result[0]["content"] == "New prompt"
    assert result[0]["role"] == "system"
    assert len(result) == 2
    assert len(messages_no_system) == 1  # Not mutated
    print("✓ Inserts system prompt when none exists")

    # Test empty messages
    result = executor._build_messages_with_system_prompt([], "New prompt")
    assert len(result) == 1
    assert result[0]["content"] == "New prompt"
    print("✓ Handles empty messages")

    print("\n" + "=" * 60)
    print("_build_messages_with_system_prompt TEST PASSED ✓")
    print("=" * 60)


if __name__ == "__main__":
    test_thinking_trace_passes_full_history()
    test_build_messages_with_system_prompt()

    print("\n" + "=" * 60)
    print("ALL INTEGRATION TESTS PASSED ✓")
    print("=" * 60)
