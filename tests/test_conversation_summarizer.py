"""Tests for incremental conversation summarization."""

import pytest
from swecli.core.context_engineering.memory.conversation_summarizer import (
    ConversationSummarizer,
    ConversationSummary,
)


def make_messages(n: int, with_system: bool = False):
    """Helper to create test messages."""
    messages = []
    if with_system:
        messages.append({"role": "system", "content": "You are a helpful assistant."})
    for i in range(n):
        messages.append({"role": "user", "content": f"User message {i+1}"})
        messages.append({"role": "assistant", "content": f"Assistant response {i+1}"})
    return messages


class TestConversationSummarizer:
    """Tests for ConversationSummarizer."""

    def test_needs_regeneration_no_cache(self):
        """Should need regeneration when no cache exists."""
        summarizer = ConversationSummarizer()
        assert summarizer.needs_regeneration(10) is True

    def test_needs_regeneration_threshold(self):
        """Should respect regeneration threshold."""
        summarizer = ConversationSummarizer(regenerate_threshold=5)
        # Manually set cache
        summarizer._cache = ConversationSummary(
            summary="test",
            message_count=10,
            last_summarized_index=4,
        )
        # 4 new messages - below threshold
        assert summarizer.needs_regeneration(14) is False
        # 5 new messages - at threshold
        assert summarizer.needs_regeneration(15) is True

    def test_get_cached_summary(self):
        """Should return cached summary."""
        summarizer = ConversationSummarizer()
        assert summarizer.get_cached_summary() is None

        summarizer._cache = ConversationSummary(
            summary="cached summary",
            message_count=10,
            last_summarized_index=4,
        )
        assert summarizer.get_cached_summary() == "cached summary"

    def test_generate_summary_not_enough_history(self):
        """Should return empty when not enough history to summarize."""
        summarizer = ConversationSummarizer(exclude_last_n=6)

        # Only 3 user/assistant pairs = 6 messages, which equals exclude_last_n
        messages = make_messages(3)
        mock_llm = lambda m, t: {"success": True, "content": "summary"}

        result = summarizer.generate_summary(messages, mock_llm)
        assert result == ""

    def test_generate_summary_initial(self):
        """Should create initial summary from all eligible messages."""
        summarizer = ConversationSummarizer(exclude_last_n=2)

        # 10 messages total, exclude last 2 = 8 messages to summarize
        messages = make_messages(5)  # 10 messages
        calls = []

        def mock_llm(msgs, task_monitor):
            calls.append(msgs)
            return {"success": True, "content": "Initial summary created"}

        result = summarizer.generate_summary(messages, mock_llm)

        assert result == "Initial summary created"
        assert len(calls) == 1
        # Should have previous_summary placeholder as "(No previous summary)"
        prompt = calls[0][1]["content"]
        assert "(No previous summary)" in prompt
        # Cache should be updated
        assert summarizer._cache is not None
        assert summarizer._cache.last_summarized_index == 8  # 10 - 2

    def test_generate_summary_incremental(self):
        """Should only send new messages on incremental update."""
        summarizer = ConversationSummarizer(exclude_last_n=2)

        # Initial summary: 10 messages, summarized first 8 (indices 0-7)
        messages = make_messages(5)  # 10 messages (indices 0-9)
        summarizer._cache = ConversationSummary(
            summary="Previous summary content",
            message_count=10,
            last_summarized_index=8,  # Already summarized up to index 8
        )

        # Add 6 more messages (3 pairs) - indices 10-15
        messages.extend([
            {"role": "user", "content": "User message 6"},
            {"role": "assistant", "content": "Assistant response 6"},
            {"role": "user", "content": "User message 7"},
            {"role": "assistant", "content": "Assistant response 7"},
            {"role": "user", "content": "User message 8"},
            {"role": "assistant", "content": "Assistant response 8"},
        ])

        calls = []

        def mock_llm(msgs, task_monitor):
            calls.append(msgs)
            return {"success": True, "content": "Updated summary"}

        result = summarizer.generate_summary(messages, mock_llm)

        assert result == "Updated summary"
        assert len(calls) == 1

        prompt = calls[0][1]["content"]
        # Should include previous summary
        assert "Previous summary content" in prompt
        # Should contain new messages (6, 7) but not message 8 (in exclude_last_n window)
        # Total 16 messages, exclude_last_n=2, so end_index=14
        # new_messages = filtered[8:14] = messages 5, 6, 7 (user + assistant for each)
        assert "User message 6" in prompt
        assert "User message 7" in prompt
        # Message 8 is in the exclude_last_n window (indices 14-15)
        assert "User message 8" not in prompt
        # Should NOT include messages already summarized (1-4)
        assert "User message 1" not in prompt
        assert "User message 4" not in prompt

    def test_generate_summary_excludes_system_messages(self):
        """System messages should be filtered out."""
        summarizer = ConversationSummarizer(exclude_last_n=2)

        messages = make_messages(5, with_system=True)  # 11 messages (1 system + 10)
        calls = []

        def mock_llm(msgs, task_monitor):
            calls.append(msgs)
            return {"success": True, "content": "Summary"}

        summarizer.generate_summary(messages, mock_llm)

        prompt = calls[0][1]["content"]
        # System message content should not appear in the prompt
        assert "You are a helpful assistant" not in prompt

    def test_generate_summary_no_new_messages(self):
        """Should return cached summary when no new messages to summarize."""
        summarizer = ConversationSummarizer(exclude_last_n=2)

        messages = make_messages(5)  # 10 messages
        summarizer._cache = ConversationSummary(
            summary="Existing summary",
            message_count=10,
            last_summarized_index=8,  # Already at the end (10 - 2)
        )

        calls = []

        def mock_llm(msgs, task_monitor):
            calls.append(msgs)
            return {"success": True, "content": "New summary"}

        result = summarizer.generate_summary(messages, mock_llm)

        # Should return cached summary, no LLM call
        assert result == "Existing summary"
        assert len(calls) == 0

    def test_generate_summary_llm_failure(self):
        """Should return cached summary on LLM failure."""
        summarizer = ConversationSummarizer(exclude_last_n=2)
        summarizer._cache = ConversationSummary(
            summary="Fallback summary",
            message_count=5,
            last_summarized_index=3,
        )

        messages = make_messages(5)

        def mock_llm(msgs, task_monitor):
            return {"success": False, "error": "API error"}

        result = summarizer.generate_summary(messages, mock_llm)
        assert result == "Fallback summary"

    def test_generate_summary_llm_exception(self):
        """Should return cached summary on LLM exception."""
        summarizer = ConversationSummarizer(exclude_last_n=2)
        summarizer._cache = ConversationSummary(
            summary="Fallback summary",
            message_count=5,
            last_summarized_index=3,
        )

        messages = make_messages(5)

        def mock_llm(msgs, task_monitor):
            raise Exception("Network error")

        result = summarizer.generate_summary(messages, mock_llm)
        assert result == "Fallback summary"

    def test_clear_cache(self):
        """Should clear the cached summary."""
        summarizer = ConversationSummarizer()
        summarizer._cache = ConversationSummary(
            summary="test",
            message_count=10,
            last_summarized_index=8,
        )
        summarizer.clear_cache()
        assert summarizer._cache is None

    def test_format_conversation_user_messages(self):
        """Should format user messages correctly."""
        summarizer = ConversationSummarizer()
        messages = [{"role": "user", "content": "Hello world"}]
        result = summarizer._format_conversation(messages)
        assert result == "User: Hello world"

    def test_format_conversation_truncates_long_messages(self):
        """Should truncate long messages to 200 chars."""
        summarizer = ConversationSummarizer()
        long_content = "x" * 300
        messages = [{"role": "user", "content": long_content}]
        result = summarizer._format_conversation(messages)
        assert len(result) == len("User: ") + 200

    def test_format_conversation_tool_calls(self):
        """Should format tool calls as tool names."""
        summarizer = ConversationSummarizer()
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"function": {"name": "read_file"}},
                    {"function": {"name": "write_file"}},
                ],
            }
        ]
        result = summarizer._format_conversation(messages)
        assert result == "Assistant: [Called tools: read_file, write_file]"

    def test_format_conversation_tool_result(self):
        """Should format tool results briefly."""
        summarizer = ConversationSummarizer()
        messages = [{"role": "tool", "content": "file contents here..."}]
        result = summarizer._format_conversation(messages)
        assert result == "Tool: [result received]"
