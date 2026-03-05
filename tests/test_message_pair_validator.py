"""Tests for MessagePairValidator — message pair integrity enforcement."""

import pytest

from swecli.core.context_engineering.message_pair_validator import (
    MessagePairValidator,
    ValidationResult,
    ViolationType,
)


class TestValidateToolResultsComplete:
    """Tests for the pre-batch-add guard in ReactExecutor."""

    def test_all_results_present(self):
        """No-op when every tool_call already has a result."""
        tool_calls = [
            {"id": "tc_1", "function": {"name": "read_file"}},
            {"id": "tc_2", "function": {"name": "write_file"}},
        ]
        results = {
            "tc_1": {"success": True, "output": "data"},
            "tc_2": {"success": True, "output": "ok"},
        }
        out = MessagePairValidator.validate_tool_results_complete(tool_calls, results)
        assert out is results  # same dict, not a copy
        assert len(out) == 2
        assert "synthetic" not in out["tc_1"]

    def test_missing_result_filled(self):
        """Missing tool result gets a synthetic error entry."""
        tool_calls = [
            {"id": "tc_1", "function": {"name": "read_file"}},
            {"id": "tc_2", "function": {"name": "bash_execute"}},
        ]
        results = {"tc_1": {"success": True, "output": "data"}}
        out = MessagePairValidator.validate_tool_results_complete(tool_calls, results)
        assert "tc_2" in out
        assert out["tc_2"]["success"] is False
        assert out["tc_2"]["synthetic"] is True
        assert "bash_execute" in out["tc_2"]["error"]

    def test_empty_tool_calls(self):
        """No tool calls means nothing to fill."""
        results = {}
        out = MessagePairValidator.validate_tool_results_complete([], results)
        assert out == {}

    def test_missing_id_skipped(self):
        """Tool call with empty id is skipped gracefully."""
        tool_calls = [{"id": "", "function": {"name": "read_file"}}]
        results = {}
        out = MessagePairValidator.validate_tool_results_complete(tool_calls, results)
        assert out == {}

    def test_all_missing_after_interrupt(self):
        """All results missing (e.g., cancelled parallel execution)."""
        tool_calls = [
            {"id": "tc_1", "function": {"name": "spawn_subagent"}},
            {"id": "tc_2", "function": {"name": "spawn_subagent"}},
            {"id": "tc_3", "function": {"name": "spawn_subagent"}},
        ]
        results = {}
        out = MessagePairValidator.validate_tool_results_complete(tool_calls, results)
        assert len(out) == 3
        for tc_id in ("tc_1", "tc_2", "tc_3"):
            assert out[tc_id]["synthetic"] is True


class TestValidate:
    """Tests for the full message list validator."""

    def test_valid_sequence(self):
        """Clean user -> assistant(tool_calls) -> tool -> tool sequence."""
        messages = [
            {"role": "user", "content": "hello"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc_1", "type": "function", "function": {"name": "read_file"}},
                    {"id": "tc_2", "type": "function", "function": {"name": "write_file"}},
                ],
            },
            {"role": "tool", "tool_call_id": "tc_1", "content": "data"},
            {"role": "tool", "tool_call_id": "tc_2", "content": "ok"},
            {"role": "assistant", "content": "done"},
        ]
        result = MessagePairValidator.validate(messages)
        assert result.is_valid

    def test_missing_tool_result(self):
        """Detects when a tool_call has no matching tool result."""
        messages = [
            {"role": "user", "content": "hello"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc_1", "type": "function", "function": {"name": "read_file"}},
                ],
            },
            # No tool result for tc_1
            {"role": "assistant", "content": "done"},
        ]
        result = MessagePairValidator.validate(messages)
        assert not result.is_valid
        violations = [v for v in result.violations if v.violation_type == ViolationType.MISSING_TOOL_RESULT]
        assert len(violations) == 1
        assert "tc_1" in violations[0].detail

    def test_orphaned_tool_result(self):
        """Detects tool result without a matching tool_call."""
        messages = [
            {"role": "user", "content": "hello"},
            {"role": "tool", "tool_call_id": "tc_orphan", "content": "stale data"},
            {"role": "assistant", "content": "done"},
        ]
        result = MessagePairValidator.validate(messages)
        assert not result.is_valid
        violations = [v for v in result.violations if v.violation_type == ViolationType.ORPHANED_TOOL_RESULT]
        assert len(violations) == 1

    def test_consecutive_same_role_warning(self):
        """Detects consecutive user messages (warning only)."""
        messages = [
            {"role": "user", "content": "hello"},
            {"role": "user", "content": "are you there?"},
            {"role": "assistant", "content": "yes"},
        ]
        result = MessagePairValidator.validate(messages)
        # Not "valid" because there's a violation, but it's just a warning
        violations = [v for v in result.violations if v.violation_type == ViolationType.CONSECUTIVE_SAME_ROLE]
        assert len(violations) == 1

    def test_consecutive_tool_not_flagged(self):
        """Consecutive tool results are normal and not flagged."""
        messages = [
            {"role": "user", "content": "hello"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc_1", "type": "function", "function": {"name": "read_file"}},
                    {"id": "tc_2", "type": "function", "function": {"name": "write_file"}},
                ],
            },
            {"role": "tool", "tool_call_id": "tc_1", "content": "data"},
            {"role": "tool", "tool_call_id": "tc_2", "content": "ok"},
        ]
        result = MessagePairValidator.validate(messages)
        consecutive = [v for v in result.violations if v.violation_type == ViolationType.CONSECUTIVE_SAME_ROLE]
        assert len(consecutive) == 0

    def test_empty_messages(self):
        """Empty list is valid."""
        result = MessagePairValidator.validate([])
        assert result.is_valid

    def test_no_tool_calls_in_assistant(self):
        """Assistant without tool_calls is fine."""
        messages = [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi there"},
        ]
        result = MessagePairValidator.validate(messages)
        assert result.is_valid


class TestRepair:
    """Tests for the repair method."""

    def test_clean_messages_unchanged(self):
        """Clean messages are returned as-is (copy)."""
        messages = [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"},
        ]
        repaired, vr = MessagePairValidator.repair(messages)
        assert len(repaired) == 2
        assert not vr.repaired

    def test_missing_tool_result_repaired(self):
        """Missing tool result gets a synthetic insertion."""
        messages = [
            {"role": "user", "content": "hello"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc_1", "type": "function", "function": {"name": "read_file"}},
                ],
            },
            {"role": "assistant", "content": "done"},
        ]
        repaired, vr = MessagePairValidator.repair(messages)
        assert vr.repaired
        # Should have: user, assistant(tc), synthetic tool, assistant
        assert len(repaired) == 4
        tool_msg = repaired[2]
        assert tool_msg["role"] == "tool"
        assert tool_msg["tool_call_id"] == "tc_1"
        assert "lost" in tool_msg["content"].lower()

    def test_orphaned_tool_result_removed(self):
        """Orphaned tool result is removed."""
        messages = [
            {"role": "user", "content": "hello"},
            {"role": "tool", "tool_call_id": "tc_orphan", "content": "stale"},
            {"role": "assistant", "content": "done"},
        ]
        repaired, vr = MessagePairValidator.repair(messages)
        assert vr.repaired
        assert len(repaired) == 2  # user + assistant (orphan removed)
        roles = [m["role"] for m in repaired]
        assert "tool" not in roles

    def test_mixed_repairs(self):
        """Both missing and orphaned violations repaired in one pass."""
        messages = [
            {"role": "user", "content": "hello"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc_1", "type": "function", "function": {"name": "read_file"}},
                ],
            },
            # tc_1 result is missing, orphan follows
            {"role": "tool", "tool_call_id": "tc_orphan", "content": "stale"},
            {"role": "assistant", "content": "done"},
        ]
        repaired, vr = MessagePairValidator.repair(messages)
        assert vr.repaired
        # user, assistant(tc), synthetic_tool(tc_1), assistant
        # orphan removed
        roles = [m["role"] for m in repaired]
        assert roles.count("tool") == 1
        tool_msg = [m for m in repaired if m["role"] == "tool"][0]
        assert tool_msg["tool_call_id"] == "tc_1"

    def test_repair_preserves_existing_tool_results(self):
        """When one of two tool_calls is missing, the existing one is preserved."""
        messages = [
            {"role": "user", "content": "hello"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc_1", "type": "function", "function": {"name": "read_file"}},
                    {"id": "tc_2", "type": "function", "function": {"name": "write_file"}},
                ],
            },
            {"role": "tool", "tool_call_id": "tc_1", "content": "file data"},
            # tc_2 result missing
            {"role": "assistant", "content": "done"},
        ]
        repaired, vr = MessagePairValidator.repair(messages)
        assert vr.repaired
        tool_msgs = [m for m in repaired if m["role"] == "tool"]
        assert len(tool_msgs) == 2
        ids = {m["tool_call_id"] for m in tool_msgs}
        assert ids == {"tc_1", "tc_2"}
        # Existing result preserved
        tc1_msg = [m for m in tool_msgs if m["tool_call_id"] == "tc_1"][0]
        assert tc1_msg["content"] == "file data"

    def test_repair_idempotent(self):
        """Repairing an already-repaired list produces same result."""
        messages = [
            {"role": "user", "content": "hello"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "tc_1", "type": "function", "function": {"name": "read_file"}},
                ],
            },
            {"role": "assistant", "content": "done"},
        ]
        repaired1, _ = MessagePairValidator.repair(messages)
        repaired2, vr2 = MessagePairValidator.repair(repaired1)
        assert not vr2.repaired  # second pass should find nothing to fix
        assert len(repaired1) == len(repaired2)
