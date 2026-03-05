"""E2E test for session persistence with new fields (Phase 1 + 2)."""

import json
import os
import sys
import tempfile
from pathlib import Path
from unittest.mock import MagicMock

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from rich.console import Console

from swecli.core.context_engineering.history import SessionManager
from swecli.core.context_engineering.memory.conversation_summarizer import (
    ConversationSummarizer,
    ConversationSummary,
)
from swecli.models.message import ChatMessage, Role
from swecli.repl.react_executor import ReactExecutor


def test_chat_message_new_fields():
    """Test that ChatMessage correctly handles new persistence fields."""
    print("\n" + "=" * 60)
    print("TEST 1: ChatMessage New Fields")
    print("=" * 60)

    # Create message with all new fields
    msg = ChatMessage(
        role=Role.ASSISTANT,
        content="I'll help you with that.",
        thinking_trace="I need to analyze the user's request...",
        reasoning_content="Let me think step by step...",
        token_usage={"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150},
    )

    # Verify fields are set
    assert msg.thinking_trace == "I need to analyze the user's request..."
    assert msg.reasoning_content == "Let me think step by step..."
    assert msg.token_usage == {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150}
    print("✓ New fields set correctly")

    # Test serialization
    msg_dict = msg.model_dump()
    assert "thinking_trace" in msg_dict
    assert "reasoning_content" in msg_dict
    assert "token_usage" in msg_dict
    print("✓ Fields serialize to dict")

    # Test deserialization
    restored_msg = ChatMessage(**msg_dict)
    assert restored_msg.thinking_trace == msg.thinking_trace
    assert restored_msg.reasoning_content == msg.reasoning_content
    assert restored_msg.token_usage == msg.token_usage
    print("✓ Fields deserialize from dict")

    # Test optional fields (default to None)
    msg_minimal = ChatMessage(role=Role.USER, content="Hello")
    assert msg_minimal.thinking_trace is None
    assert msg_minimal.reasoning_content is None
    assert msg_minimal.token_usage is None
    print("✓ Optional fields default to None")

    print("\nTEST 1 PASSED ✓")


def test_conversation_summarizer_serialization():
    """Test ConversationSummarizer to_dict() and load_from_dict()."""
    print("\n" + "=" * 60)
    print("TEST 2: ConversationSummarizer Serialization")
    print("=" * 60)

    summarizer = ConversationSummarizer(regenerate_threshold=5)

    # Test empty cache
    assert summarizer.to_dict() is None
    print("✓ Empty cache returns None")

    # Set up cache manually
    summarizer._cache = ConversationSummary(
        summary="Test conversation summary about coding tasks.",
        message_count=12,
        last_summarized_index=8,
    )

    # Test to_dict
    cache_dict = summarizer.to_dict()
    assert cache_dict is not None
    assert cache_dict["summary"] == "Test conversation summary about coding tasks."
    assert cache_dict["message_count"] == 12
    assert cache_dict["last_summarized_index"] == 8
    print("✓ to_dict() returns correct structure")

    # Test load_from_dict
    new_summarizer = ConversationSummarizer(regenerate_threshold=5)
    new_summarizer.load_from_dict(cache_dict)

    assert new_summarizer._cache is not None
    assert new_summarizer._cache.summary == "Test conversation summary about coding tasks."
    assert new_summarizer._cache.message_count == 12
    assert new_summarizer._cache.last_summarized_index == 8
    print("✓ load_from_dict() restores cache correctly")

    # Test load_from_dict with None
    new_summarizer.load_from_dict(None)
    assert new_summarizer._cache is None
    print("✓ load_from_dict(None) clears cache")

    # Test load_from_dict with empty dict (edge case - treats as no cache)
    new_summarizer.load_from_dict({})
    assert new_summarizer._cache is None  # Empty dict is falsy, clears cache
    print("✓ load_from_dict({}) clears cache (empty dict is falsy)")

    print("\nTEST 2 PASSED ✓")


def test_full_session_persistence_round_trip():
    """Test that all new fields are persisted and restored through session save/load."""
    print("\n" + "=" * 60)
    print("TEST 3: Full Session Persistence Round-Trip")
    print("=" * 60)

    with tempfile.TemporaryDirectory() as tmpdir:
        session_dir = Path(tmpdir)
        session_manager = SessionManager(session_dir=session_dir)
        session_manager.create_session(working_directory=str(Path.cwd()))

        # Create ReactExecutor with mocked dependencies
        mock_config = MagicMock()
        mock_config.auto_save_interval = 1

        executor = ReactExecutor(
            console=Console(force_terminal=True),
            session_manager=session_manager,
            config=mock_config,
            llm_caller=MagicMock(),
            tool_executor=MagicMock(),
        )

        # Simulate iteration data
        executor._current_thinking_trace = "I need to analyze the user's request..."
        executor._current_reasoning_content = "Let me think step by step..."
        executor._current_token_usage = {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150,
        }

        # Set up summarizer cache manually
        executor._conversation_summarizer._cache = ConversationSummary(
            summary="Test conversation summary",
            message_count=12,
            last_summarized_index=8,
        )

        # STEP 1: Add message with new fields
        session = session_manager.current_session
        msg = ChatMessage(
            role=Role.ASSISTANT,
            content="I'll help you with that.",
            thinking_trace=executor._current_thinking_trace,
            reasoning_content=executor._current_reasoning_content,
            token_usage=executor._current_token_usage,
        )
        session.add_message(msg)

        # Save summarizer cache to metadata
        cache_data = executor._conversation_summarizer.to_dict()
        session.metadata["conversation_summary"] = cache_data

        # Save session
        session_manager.save_session()

        session_file = session_dir / f"{session.id}.json"
        print(f"Session saved to: {session_file}")

        # STEP 2: Verify JSON contains all fields
        with open(session_file) as f:
            data = json.load(f)

        last_msg = data["messages"][-1]
        assert "thinking_trace" in last_msg, "thinking_trace not in JSON"
        assert last_msg["thinking_trace"] == "I need to analyze the user's request..."
        assert "reasoning_content" in last_msg, "reasoning_content not in JSON"
        assert last_msg["reasoning_content"] == "Let me think step by step..."
        assert "token_usage" in last_msg, "token_usage not in JSON"
        assert last_msg["token_usage"]["total_tokens"] == 150
        print("✓ All new message fields saved to JSON")

        assert "conversation_summary" in data["metadata"], "conversation_summary not in metadata"
        assert data["metadata"]["conversation_summary"]["summary"] == "Test conversation summary"
        print("✓ Summarizer cache saved to metadata")

        # STEP 3: Simulate session reload
        new_session_manager = SessionManager(session_dir=session_dir)
        new_session_manager.load_session(session.id)

        # Create new ReactExecutor (simulates app restart)
        new_executor = ReactExecutor(
            console=Console(force_terminal=True),
            session_manager=new_session_manager,
            config=mock_config,
            llm_caller=MagicMock(),
            tool_executor=MagicMock(),
        )

        # Verify summarizer cache was restored
        cache = new_executor._conversation_summarizer._cache
        assert cache is not None, "Summarizer cache not restored"
        assert cache.summary == "Test conversation summary", f"Wrong summary: {cache.summary}"
        assert cache.message_count == 12, f"Wrong message_count: {cache.message_count}"
        print("✓ Summarizer cache restored from session")

        # Verify message fields were restored
        restored_msg = new_session_manager.current_session.messages[-1]
        assert restored_msg.thinking_trace == "I need to analyze the user's request..."
        assert restored_msg.reasoning_content == "Let me think step by step..."
        assert restored_msg.token_usage == {
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150,
        }
        print("✓ All message fields restored correctly")

        print("\nTEST 3 PASSED ✓")


def test_backward_compatibility():
    """Test that old sessions without new fields load correctly."""
    print("\n" + "=" * 60)
    print("TEST 4: Backward Compatibility")
    print("=" * 60)

    with tempfile.TemporaryDirectory() as tmpdir:
        session_dir = Path(tmpdir)

        # Create an "old" session file without new fields
        old_session_data = {
            "id": "test123",
            "created_at": "2024-01-01T00:00:00",
            "updated_at": "2024-01-01T00:00:00",
            "messages": [
                {
                    "role": "assistant",
                    "content": "Hello!",
                    "timestamp": "2024-01-01T00:00:00",
                    "metadata": {},
                    "tool_calls": [],
                    "tokens": None,
                    # Note: NO thinking_trace, reasoning_content, token_usage
                }
            ],
            "context_files": [],
            "working_directory": None,
            "metadata": {},  # Note: NO conversation_summary
            "playbook": {},
            "file_changes": [],
        }

        # Write old session file
        session_file = session_dir / "test123.json"
        session_dir.mkdir(parents=True, exist_ok=True)
        with open(session_file, "w") as f:
            json.dump(old_session_data, f)

        # Load session
        session_manager = SessionManager(session_dir=session_dir)
        session_manager.load_session("test123")

        # Verify old messages load with None for new fields
        msg = session_manager.current_session.messages[0]
        assert msg.thinking_trace is None, "thinking_trace should be None for old sessions"
        assert msg.reasoning_content is None, "reasoning_content should be None for old sessions"
        assert msg.token_usage is None, "token_usage should be None for old sessions"
        print("✓ Old messages load with None for new fields")

        # Create ReactExecutor - should handle missing conversation_summary
        mock_config = MagicMock()
        mock_config.auto_save_interval = 1

        executor = ReactExecutor(
            console=Console(force_terminal=True),
            session_manager=session_manager,
            config=mock_config,
            llm_caller=MagicMock(),
            tool_executor=MagicMock(),
        )

        # Verify summarizer starts with no cache (no error)
        assert executor._conversation_summarizer._cache is None
        print("✓ ReactExecutor handles missing conversation_summary gracefully")

        print("\nTEST 4 PASSED ✓")


def main():
    """Run all tests."""
    print("\n" + "=" * 60)
    print("SESSION PERSISTENCE E2E TESTS (Phase 1 + 2)")
    print("=" * 60)

    test_chat_message_new_fields()
    test_conversation_summarizer_serialization()
    test_full_session_persistence_round_trip()
    test_backward_compatibility()

    print("\n" + "=" * 60)
    print("ALL SESSION PERSISTENCE TESTS PASSED ✓")
    print("=" * 60 + "\n")


if __name__ == "__main__":
    main()
