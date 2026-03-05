"""Tests for staged compaction, observation masking, artifact index, and history archival."""

import tempfile
from pathlib import Path
from unittest.mock import MagicMock, patch

from swecli.core.context_engineering.compaction import (
    STAGE_COMPACT,
    STAGE_AGGRESSIVE,
    STAGE_MASK,
    STAGE_WARNING,
    ArtifactIndex,
    ContextCompactor,
    OptimizationLevel,
)
from swecli.models.config import AppConfig


def _make_compactor(max_context_tokens: int = 100_000) -> ContextCompactor:
    config = AppConfig()
    config.max_context_tokens = max_context_tokens
    config.model = "gpt-4"
    mock_client = MagicMock()
    with patch.object(AppConfig, "get_model_info", return_value=None):
        return ContextCompactor(config, mock_client)


def _make_messages(count: int) -> list[dict]:
    msgs = [{"role": "system", "content": "You are a helpful assistant."}]
    for i in range(count):
        role = "user" if i % 2 == 0 else "assistant"
        msgs.append({"role": role, "content": f"Message {i}: {'x' * 200}"})
    return msgs


# ---------------------------------------------------------------------------
# Staged thresholds
# ---------------------------------------------------------------------------


class TestStagedThresholds:
    """Verify threshold constants and optimization levels."""

    def test_threshold_ordering(self) -> None:
        assert STAGE_WARNING < STAGE_MASK < STAGE_AGGRESSIVE < STAGE_COMPACT

    def test_threshold_values(self) -> None:
        assert STAGE_WARNING == 0.70
        assert STAGE_MASK == 0.80
        assert STAGE_AGGRESSIVE == 0.90
        assert STAGE_COMPACT == 0.99


class TestCheckUsage:
    """check_usage returns the correct optimization level."""

    def test_none_when_low(self) -> None:
        compactor = _make_compactor(max_context_tokens=1_000_000)
        messages = _make_messages(3)
        level = compactor.check_usage(messages, "System")
        assert level == OptimizationLevel.NONE

    def test_warning_at_70_pct(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        # Simulate 70% via API calibration
        compactor.update_from_api_usage(72, 5)
        level = compactor.check_usage([], "")
        assert level == OptimizationLevel.WARNING

    def test_mask_at_80_pct(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        compactor.update_from_api_usage(85, 5)
        level = compactor.check_usage([], "")
        assert level == OptimizationLevel.MASK

    def test_aggressive_at_90_pct(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        compactor.update_from_api_usage(92, 5)
        level = compactor.check_usage([], "")
        assert level == OptimizationLevel.AGGRESSIVE

    def test_compact_at_99_pct(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        compactor.update_from_api_usage(100, 5)
        level = compactor.check_usage([], "")
        assert level == OptimizationLevel.COMPACT


# ---------------------------------------------------------------------------
# Observation masking
# ---------------------------------------------------------------------------


class TestObservationMasking:
    """mask_old_observations replaces old tool results with compact refs."""

    def _make_tool_messages(self, count: int) -> list[dict]:
        """Create messages with tool results interleaved."""
        msgs = [{"role": "system", "content": "System prompt"}]
        for i in range(count):
            msgs.append({
                "role": "assistant",
                "content": "",
                "tool_calls": [{"id": f"call_{i}", "function": {"name": "read_file", "arguments": "{}"}}],
            })
            msgs.append({
                "role": "tool",
                "tool_call_id": f"call_{i}",
                "content": f"File content for call {i} " + "x" * 200,
            })
        return msgs

    def test_mask_keeps_recent_tool_results(self) -> None:
        compactor = _make_compactor()
        msgs = self._make_tool_messages(10)
        compactor.mask_old_observations(msgs, OptimizationLevel.MASK)

        # Recent 6 tool results should be intact
        tool_msgs = [m for m in msgs if m.get("role") == "tool"]
        recent = tool_msgs[-6:]
        for m in recent:
            assert not m["content"].startswith("[ref:")

    def test_mask_replaces_old_tool_results(self) -> None:
        compactor = _make_compactor()
        msgs = self._make_tool_messages(10)
        compactor.mask_old_observations(msgs, OptimizationLevel.MASK)

        tool_msgs = [m for m in msgs if m.get("role") == "tool"]
        old = tool_msgs[:-6]
        for m in old:
            assert m["content"].startswith("[ref:")

    def test_aggressive_keeps_only_3(self) -> None:
        compactor = _make_compactor()
        msgs = self._make_tool_messages(10)
        compactor.mask_old_observations(msgs, OptimizationLevel.AGGRESSIVE)

        tool_msgs = [m for m in msgs if m.get("role") == "tool"]
        recent = tool_msgs[-3:]
        old = tool_msgs[:-3]
        for m in recent:
            assert not m["content"].startswith("[ref:")
        for m in old:
            assert m["content"].startswith("[ref:")

    def test_no_masking_for_none_level(self) -> None:
        compactor = _make_compactor()
        msgs = self._make_tool_messages(10)
        original_contents = [m.get("content", "") for m in msgs if m.get("role") == "tool"]
        compactor.mask_old_observations(msgs, OptimizationLevel.NONE)
        current_contents = [m.get("content", "") for m in msgs if m.get("role") == "tool"]
        assert original_contents == current_contents

    def test_already_masked_messages_skipped(self) -> None:
        compactor = _make_compactor()
        msgs = self._make_tool_messages(10)
        # Mask once
        compactor.mask_old_observations(msgs, OptimizationLevel.MASK)
        # Mask again — should not double-mask
        compactor.mask_old_observations(msgs, OptimizationLevel.MASK)
        tool_msgs = [m for m in msgs if m.get("role") == "tool"]
        for m in tool_msgs:
            # Should not have [ref: [ref: ...]]
            assert m["content"].count("[ref:") <= 1

    def test_few_tool_results_not_masked(self) -> None:
        compactor = _make_compactor()
        msgs = self._make_tool_messages(3)
        original_contents = [m.get("content", "") for m in msgs if m.get("role") == "tool"]
        compactor.mask_old_observations(msgs, OptimizationLevel.MASK)
        current_contents = [m.get("content", "") for m in msgs if m.get("role") == "tool"]
        assert original_contents == current_contents


# ---------------------------------------------------------------------------
# Artifact index
# ---------------------------------------------------------------------------


class TestArtifactIndex:
    """ArtifactIndex tracks files touched in a session."""

    def test_record_and_summary(self) -> None:
        idx = ArtifactIndex()
        idx.record("/src/main.py", "created", "50 lines")
        idx.record("/src/utils.py", "read", "100 lines")
        summary = idx.as_summary()
        assert "/src/main.py" in summary
        assert "/src/utils.py" in summary
        assert "created" in summary
        assert "read" in summary

    def test_merge_updates_existing(self) -> None:
        idx = ArtifactIndex()
        idx.record("/src/main.py", "read", "50 lines")
        idx.record("/src/main.py", "modified", "+10/-5")
        summary = idx.as_summary()
        assert "read, modified" in summary
        assert len(idx) == 1

    def test_empty_summary(self) -> None:
        idx = ArtifactIndex()
        assert idx.as_summary() == ""

    def test_serialization_roundtrip(self) -> None:
        idx = ArtifactIndex()
        idx.record("/foo.py", "created", "10 lines")
        idx.record("/bar.py", "modified", "+5/-3")
        data = idx.to_dict()
        restored = ArtifactIndex.from_dict(data)
        assert len(restored) == 2
        assert restored.as_summary() == idx.as_summary()

    def test_operation_count(self) -> None:
        idx = ArtifactIndex()
        idx.record("/foo.py", "read")
        idx.record("/foo.py", "modified")
        idx.record("/foo.py", "read")
        assert idx._entries["/foo.py"]["operation_count"] == 3


# ---------------------------------------------------------------------------
# History archival
# ---------------------------------------------------------------------------


class TestHistoryArchival:
    """archive_history writes full conversation to file."""

    def test_creates_archive_file(self) -> None:
        compactor = _make_compactor()
        messages = _make_messages(5)

        with tempfile.TemporaryDirectory() as tmpdir:
            with patch.object(Path, "home", return_value=Path(tmpdir)):
                path = compactor.archive_history(messages, "test-session")

        assert path is not None
        assert "test-session" in path
        assert "history_archive_" in path

    def test_archive_contains_messages(self) -> None:
        compactor = _make_compactor()
        messages = _make_messages(5)

        with tempfile.TemporaryDirectory() as tmpdir:
            with patch.object(Path, "home", return_value=Path(tmpdir)):
                path = compactor.archive_history(messages, "test-session")
                content = Path(path).read_text()

        assert "Message 0" in content
        assert "Messages: 6" in content  # system + 5

    def test_archive_returns_none_on_failure(self) -> None:
        compactor = _make_compactor()
        messages = _make_messages(3)

        with patch.object(Path, "mkdir", side_effect=OSError("Permission denied")):
            path = compactor.archive_history(messages, "test-session")
        assert path is None

    def test_compact_includes_archive_reference(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        # Force fallback summary (LLM mock returns failure)
        compactor._http_client.post_json.return_value = MagicMock(success=False)
        messages = _make_messages(20)

        with tempfile.TemporaryDirectory() as tmpdir:
            with patch.object(Path, "home", return_value=Path(tmpdir)):
                result = compactor.compact(messages, "System prompt")

        summary_msg = result[1]
        assert "archived at" in summary_msg["content"].lower()

    def test_compact_includes_artifact_index(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        # Force fallback summary
        compactor._http_client.post_json.return_value = MagicMock(success=False)
        compactor.artifact_index.record("/src/main.py", "modified", "+10/-5")
        messages = _make_messages(20)

        with tempfile.TemporaryDirectory() as tmpdir:
            with patch.object(Path, "home", return_value=Path(tmpdir)):
                result = compactor.compact(messages, "System prompt")

        summary_msg = result[1]
        assert "/src/main.py" in summary_msg["content"]
        assert "Artifact Index" in summary_msg["content"]


# ---------------------------------------------------------------------------
# Stage warning deduplication
# ---------------------------------------------------------------------------


class TestWarningDedup:
    """Stage warnings only fire once per stage."""

    def test_warning_fires_once(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        compactor.update_from_api_usage(72, 5)

        level1 = compactor.check_usage([], "")
        assert level1 == OptimizationLevel.WARNING
        assert compactor._warned_70 is True

        # Second call should still return WARNING but not re-log
        level2 = compactor.check_usage([], "")
        assert level2 == OptimizationLevel.WARNING

    def test_warnings_reset_after_compaction(self) -> None:
        compactor = _make_compactor(max_context_tokens=100)
        compactor._warned_70 = True
        compactor._warned_80 = True
        compactor._warned_90 = True

        messages = _make_messages(20)
        with tempfile.TemporaryDirectory() as tmpdir:
            with patch.object(Path, "home", return_value=Path(tmpdir)):
                compactor.compact(messages, "System prompt")

        assert compactor._warned_70 is False
        assert compactor._warned_80 is False
        assert compactor._warned_90 is False
