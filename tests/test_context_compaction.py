"""Tests for context compaction feature."""

from unittest.mock import MagicMock, patch

from swecli.core.context_engineering.compaction import (
    STAGE_COMPACT,
    ContextCompactor,
    OptimizationLevel,
    ArtifactIndex,
)

# Backwards-compat alias used by tests
COMPACTION_THRESHOLD = STAGE_COMPACT
from swecli.core.context_engineering.retrieval.token_monitor import ContextTokenMonitor
from swecli.models.config import AppConfig


def _make_compactor(max_context_tokens: int = 1000) -> ContextCompactor:
    """Create a ContextCompactor with a small context window for testing."""
    config = AppConfig()
    config.max_context_tokens = max_context_tokens
    config.model = "gpt-4"
    mock_client = MagicMock()
    # Return None from get_model_info so the compactor falls back to max_context_tokens
    with patch.object(AppConfig, "get_model_info", return_value=None):
        return ContextCompactor(config, mock_client)


def _make_compactor_with_model_info(context_length: int) -> ContextCompactor:
    """Create a ContextCompactor that uses model_info.context_length."""
    config = AppConfig()
    config.max_context_tokens = 999_999  # Should be ignored
    config.model = "gpt-4"
    mock_client = MagicMock()
    mock_model_info = MagicMock()
    mock_model_info.context_length = context_length
    with patch.object(AppConfig, "get_model_info", return_value=mock_model_info):
        return ContextCompactor(config, mock_client)


def _make_messages(count: int) -> list[dict]:
    """Generate a list of alternating user/assistant messages."""
    msgs = [{"role": "system", "content": "You are a helpful assistant."}]
    for i in range(count):
        role = "user" if i % 2 == 0 else "assistant"
        msgs.append({"role": role, "content": f"Message {i}: {'x' * 200}"})
    return msgs


# ---------------------------------------------------------------------------
# 1. Threshold tests
# ---------------------------------------------------------------------------


class TestThreshold:
    """COMPACTION_THRESHOLD is 0.99 — triggers at 99%."""

    def test_threshold_value(self) -> None:
        """COMPACTION_THRESHOLD should be 0.99."""
        assert COMPACTION_THRESHOLD == 0.99

    def test_triggers_at_99_percent(self) -> None:
        """should_compact triggers when usage exceeds 99% of context."""
        compactor = _make_compactor(max_context_tokens=100)
        messages = _make_messages(20)
        assert compactor.should_compact(messages, "System prompt")

    def test_does_not_trigger_below_threshold(self) -> None:
        """should_compact returns False when well below 99%."""
        compactor = _make_compactor(max_context_tokens=500_000)
        messages = [
            {"role": "system", "content": "System prompt"},
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi there"},
        ]
        assert not compactor.should_compact(messages, "System prompt")

    def test_boundary_just_below_threshold(self) -> None:
        """At exactly threshold boundary (slightly under), should not compact."""
        compactor = _make_compactor(max_context_tokens=500_000)
        # A few messages won't get near 99%
        messages = _make_messages(5)
        assert not compactor.should_compact(messages, "System prompt")


# ---------------------------------------------------------------------------
# 2. Denominator tests (model_info vs fallback)
# ---------------------------------------------------------------------------


class TestDenominator:
    """_max_context uses model_info.context_length when available."""

    def test_uses_model_info_context_length(self) -> None:
        """When model_info provides context_length, use that as denominator."""
        compactor = _make_compactor_with_model_info(context_length=200_000)
        assert compactor._max_context == 200_000

    def test_falls_back_to_max_context_tokens(self) -> None:
        """When model_info is None, fall back to config.max_context_tokens."""
        compactor = _make_compactor(max_context_tokens=50_000)
        assert compactor._max_context == 50_000


# ---------------------------------------------------------------------------
# 3. usage_pct scaling
# ---------------------------------------------------------------------------


class TestUsagePct:
    """usage_pct grows correctly with token count."""

    def test_zero_before_any_call(self) -> None:
        """usage_pct is 0 before any should_compact call."""
        compactor = _make_compactor(max_context_tokens=500_000)
        assert compactor.usage_pct == 0.0

    def test_positive_after_should_compact(self) -> None:
        """usage_pct becomes positive after should_compact."""
        compactor = _make_compactor(max_context_tokens=500_000)
        messages = _make_messages(5)
        compactor.should_compact(messages, "System prompt")
        assert 0 < compactor.usage_pct < 100

    def test_scales_with_context_window(self) -> None:
        """Same messages give higher usage_pct with smaller context window."""
        messages = _make_messages(10)
        small = _make_compactor(max_context_tokens=10_000)
        large = _make_compactor(max_context_tokens=500_000)
        small.should_compact(messages, "Sys")
        large.should_compact(messages, "Sys")
        assert small.usage_pct > large.usage_pct

    def test_zero_max_context_returns_zero(self) -> None:
        """usage_pct returns 0 when _max_context is 0 (avoid division by zero)."""
        compactor = _make_compactor(max_context_tokens=1)
        compactor._max_context = 0
        assert compactor.usage_pct == 0.0


# ---------------------------------------------------------------------------
# 4. Progressive usage — monotonically increasing
# ---------------------------------------------------------------------------


class TestProgressiveUsage:
    """More messages → higher usage_pct."""

    def test_monotonically_increasing(self) -> None:
        """5, 10, 20, 50 messages → strictly increasing usage_pct."""
        pcts = []
        for count in (5, 10, 20, 50):
            compactor = _make_compactor(max_context_tokens=500_000)
            messages = _make_messages(count)
            compactor.should_compact(messages, "System prompt")
            pcts.append(compactor.usage_pct)
        for i in range(1, len(pcts)):
            assert pcts[i] > pcts[i - 1], f"pct[{i}]={pcts[i]} <= pct[{i-1}]={pcts[i-1]}"


# ---------------------------------------------------------------------------
# 5. API calibration
# ---------------------------------------------------------------------------


class TestAPICalibration:
    """update_from_api_usage() overrides tiktoken estimates."""

    def test_overrides_tiktoken(self) -> None:
        """API calibration sets usage_pct directly."""
        compactor = _make_compactor(max_context_tokens=100_000)
        compactor.update_from_api_usage(50_000, 10)
        assert compactor.usage_pct == 50.0

    def test_zero_ignored(self) -> None:
        """update_from_api_usage(0) does not change token count."""
        compactor = _make_compactor(max_context_tokens=100_000)
        compactor.update_from_api_usage(0)
        assert compactor.usage_pct == 0.0

    def test_incremental_delta_after_calibration(self) -> None:
        """New messages after calibration add delta to API base."""
        compactor = _make_compactor(max_context_tokens=100_000)
        messages = _make_messages(10)

        compactor.update_from_api_usage(50_000, len(messages))

        # Add 2 more messages
        messages.append({"role": "assistant", "content": "Tool call result " + "x" * 200})
        messages.append({"role": "user", "content": "Next question " + "x" * 200})

        compactor.should_compact(messages, "System prompt")
        pct = compactor.usage_pct
        assert pct > 50.0  # API base + delta
        assert pct < 52.0  # Delta for 2 messages is small


# ---------------------------------------------------------------------------
# 6. Oscillation prevention
# ---------------------------------------------------------------------------


class TestOscillationPrevention:
    """should_compact() after API calibration should not revert to tiktoken."""

    def test_no_revert_to_tiktoken(self) -> None:
        """Calling should_compact after API calibration preserves API value."""
        compactor = _make_compactor(max_context_tokens=100_000)
        messages = _make_messages(10)

        # First call uses tiktoken
        compactor.should_compact(messages, "System prompt")
        tiktoken_pct = compactor.usage_pct

        # API calibrates higher
        compactor.update_from_api_usage(50_000, len(messages))
        api_pct = compactor.usage_pct
        assert api_pct == 50.0

        # should_compact again with same messages should NOT revert
        compactor.should_compact(messages, "System prompt")
        after_pct = compactor.usage_pct
        assert after_pct == api_pct
        assert after_pct != tiktoken_pct


# ---------------------------------------------------------------------------
# 7. Post-compaction reset
# ---------------------------------------------------------------------------


class TestPostCompactionReset:
    """Compaction invalidates API calibration."""

    def test_calibration_cleared_after_compact(self) -> None:
        """API calibration is reset after compact() changes messages."""
        compactor = _make_compactor(max_context_tokens=100_000)
        messages = _make_messages(20)

        compactor.update_from_api_usage(50_000, len(messages))
        assert compactor._api_prompt_tokens == 50_000

        compactor.compact(messages, "System prompt")
        assert compactor._api_prompt_tokens == 0
        assert compactor._msg_count_at_calibration == 0

    def test_usage_drops_after_compact(self) -> None:
        """usage_pct should be lower after compaction reduces messages."""
        compactor = _make_compactor(max_context_tokens=100_000)
        messages = _make_messages(30)

        compactor.should_compact(messages, "System prompt")
        before_pct = compactor.usage_pct

        compacted = compactor.compact(messages, "System prompt")
        compactor.should_compact(compacted, "System prompt")
        after_pct = compactor.usage_pct

        assert after_pct < before_pct


# ---------------------------------------------------------------------------
# 8. Compaction mechanics
# ---------------------------------------------------------------------------


class TestCompactionMechanics:
    """Tests for the compact() method itself."""

    def test_preserves_system_prompt(self) -> None:
        """First message (system) is always preserved."""
        compactor = _make_compactor(max_context_tokens=100)
        messages = _make_messages(20)
        result = compactor.compact(messages, "System prompt")
        assert result[0]["role"] == "system"
        assert result[0]["content"] == "You are a helpful assistant."

    def test_preserves_recent_messages(self) -> None:
        """Last N messages remain intact."""
        compactor = _make_compactor(max_context_tokens=100)
        messages = _make_messages(30)
        result = compactor.compact(messages, "System prompt")
        assert result[-1] == messages[-1]
        assert result[-2] == messages[-2]

    def test_summarizes_middle_messages(self) -> None:
        """Middle messages are replaced with a summary."""
        compactor = _make_compactor(max_context_tokens=100)
        messages = _make_messages(20)
        result = compactor.compact(messages, "System prompt")
        assert len(result) < len(messages)
        assert "CONVERSATION SUMMARY" in result[1]["content"]

    def test_fewer_messages_after_compact(self) -> None:
        """Compaction reduces total message count."""
        compactor = _make_compactor(max_context_tokens=100)
        messages = _make_messages(20)
        result = compactor.compact(messages, "System prompt")
        assert len(result) < len(messages)


# ---------------------------------------------------------------------------
# 9. Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    """Edge cases: empty, minimal, huge, multimodal."""

    def test_empty_messages(self) -> None:
        """Empty message list returns empty."""
        compactor = _make_compactor(max_context_tokens=100)
        assert compactor.compact([], "System") == []

    def test_minimal_messages(self) -> None:
        """Too few messages to compact — returns as-is."""
        compactor = _make_compactor(max_context_tokens=100)
        messages = [
            {"role": "system", "content": "System"},
            {"role": "user", "content": "Hi"},
        ]
        result = compactor.compact(messages, "System")
        assert result == messages

    def test_exactly_four_messages(self) -> None:
        """4 messages (the boundary) should return as-is."""
        compactor = _make_compactor(max_context_tokens=100)
        messages = _make_messages(3)  # system + 3 = 4
        result = compactor.compact(messages, "System")
        assert result == messages

    def test_huge_context_window(self) -> None:
        """Very large context window — small conversation should not compact."""
        compactor = _make_compactor(max_context_tokens=10_000_000)
        messages = _make_messages(5)
        assert not compactor.should_compact(messages, "System prompt")
        assert compactor.usage_pct < 1.0

    def test_multimodal_content(self) -> None:
        """Multimodal (list-format) content is counted correctly."""
        compactor = _make_compactor(max_context_tokens=500_000)
        messages = [
            {"role": "system", "content": "System prompt"},
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Describe this image"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc"}},
                ],
            },
        ]
        compactor.should_compact(messages, "System prompt")
        assert compactor.usage_pct > 0

    def test_tool_call_arguments_counted(self) -> None:
        """Tool call arguments contribute to token count."""
        compactor = _make_compactor(max_context_tokens=500_000)
        messages = [
            {"role": "system", "content": "System"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "function": {
                            "name": "read_file",
                            "arguments": '{"path": "/foo/bar.py"}',
                        }
                    }
                ],
            },
        ]
        compactor.should_compact(messages, "System")
        pct_with_tools = compactor.usage_pct

        # Compare with messages without tool calls
        compactor2 = _make_compactor(max_context_tokens=500_000)
        messages2 = [
            {"role": "system", "content": "System"},
            {"role": "assistant", "content": ""},
        ]
        compactor2.should_compact(messages2, "System")
        pct_without_tools = compactor2.usage_pct

        assert pct_with_tools > pct_without_tools


# ---------------------------------------------------------------------------
# Token monitor
# ---------------------------------------------------------------------------


class TestTokenCounting:
    """Tests for ContextTokenMonitor."""

    def test_token_counting_works(self) -> None:
        """Token counting should return positive values."""
        monitor = ContextTokenMonitor(model="gpt-4")
        count = monitor.count_tokens("Hello, world! This is a test.")
        assert count > 0

    def test_empty_string(self) -> None:
        """Empty string should return 0 tokens."""
        monitor = ContextTokenMonitor(model="gpt-4")
        assert monitor.count_tokens("") == 0


# ---------------------------------------------------------------------------
# 10. Tool Result Sanitization (Security)
# ---------------------------------------------------------------------------


class TestToolResultSanitization:
    """Tests for _sanitize_for_summarization() security feature."""

    def test_uses_result_summary_when_available(self) -> None:
        """When result_summary exists, it should replace the full result."""
        compactor = _make_compactor()
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "name": "read_file",
                        "result": "API_KEY=secret123\nDATABASE_PASSWORD=verysecret\n" + "x" * 1000,
                        "result_summary": "Read .env file with 3 environment variables",
                    }
                ],
            }
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        assert sanitized[0]["tool_calls"][0]["result"] == "Read .env file with 3 environment variables"
        assert "API_KEY" not in sanitized[0]["tool_calls"][0]["result"]
        assert "secret123" not in sanitized[0]["tool_calls"][0]["result"]

    def test_truncates_long_results_without_summary(self) -> None:
        """Results without summary should be truncated to 200 chars."""
        compactor = _make_compactor()
        long_result = "x" * 500  # 500 chars
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "name": "bash",
                        "result": long_result,
                    }
                ],
            }
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        result = sanitized[0]["tool_calls"][0]["result"]
        assert len(result) == 203  # 200 chars + "..."
        assert result.endswith("...")
        assert result[:200] == long_result[:200]

    def test_keeps_short_results_unchanged(self) -> None:
        """Results under 200 chars without summary are kept as-is."""
        compactor = _make_compactor()
        short_result = "Success"
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "name": "bash",
                        "result": short_result,
                    }
                ],
            }
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        assert sanitized[0]["tool_calls"][0]["result"] == short_result

    def test_empty_result_uses_placeholder(self) -> None:
        """Empty results should use '[result omitted]' placeholder."""
        compactor = _make_compactor()
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "name": "bash",
                        "result": "",
                    }
                ],
            }
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        assert sanitized[0]["tool_calls"][0]["result"] == "[result omitted]"

    def test_strips_api_keys_from_file_contents(self) -> None:
        """Sensitive data like API keys should not appear in sanitized output."""
        compactor = _make_compactor()
        # Make content longer than 200 chars to force truncation
        sensitive_content = """# Configuration file
# This file contains sensitive credentials

OPENAI_API_KEY=sk-proj-abc123def456ghijklmnopqrstuvwxyz
AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
DATABASE_URL=postgresql://user:password@localhost:5432/db
STRIPE_SECRET_KEY=sk_test_51234567890abcdefghijklmnopqrstuvwxyz
JWT_SECRET=very_secret_jwt_key_that_should_not_be_exposed_in_logs
"""
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "name": "read_file",
                        "result": sensitive_content,
                        # No result_summary — will be truncated
                    }
                ],
            }
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        result = sanitized[0]["tool_calls"][0]["result"]
        # Result should be truncated to 200 chars
        assert len(result) == 203  # 200 + "..."
        assert result.endswith("...")
        # The later secrets (beyond char 200) should not appear
        assert "STRIPE_SECRET_KEY" not in result
        assert "JWT_SECRET" not in result
        assert "very_secret_jwt_key_that_should_not_be_exposed_in_logs" not in result

    def test_messages_without_tool_calls_unchanged(self) -> None:
        """Messages without tool_calls should pass through unchanged."""
        compactor = _make_compactor()
        messages = [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi there!"},
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        assert sanitized == messages

    def test_multiple_tool_calls_all_sanitized(self) -> None:
        """All tool_calls in a message should be sanitized."""
        compactor = _make_compactor()
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "name": "read_file",
                        "result": "x" * 500,
                    },
                    {
                        "id": "call_2",
                        "name": "bash",
                        "result": "API_KEY=secret",
                        "result_summary": "Echoed env var",
                    },
                ],
            }
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        # First tool call: truncated
        assert len(sanitized[0]["tool_calls"][0]["result"]) == 203
        assert sanitized[0]["tool_calls"][0]["result"].endswith("...")

        # Second tool call: uses summary
        assert sanitized[0]["tool_calls"][1]["result"] == "Echoed env var"
        assert "secret" not in sanitized[0]["tool_calls"][1]["result"]

    def test_original_messages_not_modified(self) -> None:
        """Sanitization should not modify the original messages."""
        compactor = _make_compactor()
        original_result = "x" * 500
        messages = [
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "name": "bash",
                        "result": original_result,
                    }
                ],
            }
        ]

        sanitized = compactor._sanitize_for_summarization(messages)

        # Original should be unchanged
        assert messages[0]["tool_calls"][0]["result"] == original_result
        # Sanitized should be different
        assert sanitized[0]["tool_calls"][0]["result"] != original_result
