"""Tests for ValidatedMessageList — state machine, auto-repair, compaction, thread safety."""

import threading

import pytest

from swecli.core.context_engineering.validated_message_list import (
    SYNTHETIC_TOOL_RESULT,
    ValidatedMessageList,
)


class TestStateMachine:
    """Core state machine transitions."""

    def test_starts_with_no_pending(self):
        vml = ValidatedMessageList()
        assert not vml.has_pending_tools
        assert vml.pending_tool_ids == frozenset()

    def test_assistant_with_tool_calls_enters_pending(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
            {"id": "tc2", "function": {"name": "read"}},
        ]})
        assert vml.has_pending_tools
        assert vml.pending_tool_ids == frozenset({"tc1", "tc2"})

    def test_tool_result_removes_from_pending(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
        ]})
        vml.append({"role": "tool", "tool_call_id": "tc1", "content": "ok"})
        assert not vml.has_pending_tools

    def test_partial_results_leave_pending(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
            {"id": "tc2", "function": {"name": "read"}},
        ]})
        vml.append({"role": "tool", "tool_call_id": "tc1", "content": "ok"})
        assert vml.has_pending_tools
        assert vml.pending_tool_ids == frozenset({"tc2"})

    def test_all_results_clears_pending(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
            {"id": "tc2", "function": {"name": "read"}},
        ]})
        vml.append({"role": "tool", "tool_call_id": "tc1", "content": "ok"})
        vml.append({"role": "tool", "tool_call_id": "tc2", "content": "ok"})
        assert not vml.has_pending_tools

    def test_assistant_without_tool_calls_no_pending(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "Hello!"})
        assert not vml.has_pending_tools


class TestAutoComplete:
    """Auto-completion of pending tool results."""

    def test_user_message_auto_completes_pending(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
        ]})
        # User message should auto-complete tc1
        vml.append({"role": "user", "content": "hello"})
        assert not vml.has_pending_tools
        # Should have: assistant, synthetic tool result, user
        assert len(vml) == 3
        assert vml[1]["role"] == "tool"
        assert vml[1]["tool_call_id"] == "tc1"
        assert vml[1]["content"] == SYNTHETIC_TOOL_RESULT
        assert vml[2]["role"] == "user"

    def test_assistant_text_auto_completes_pending(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
        ]})
        # New assistant message (no tool_calls) should auto-complete
        vml.append({"role": "assistant", "content": "Done thinking"})
        assert not vml.has_pending_tools
        assert len(vml) == 3
        assert vml[1]["role"] == "tool"
        assert vml[1]["content"] == SYNTHETIC_TOOL_RESULT

    def test_assistant_with_tool_calls_auto_completes_previous(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
        ]})
        # New assistant message WITH tool_calls should auto-complete previous pending
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc2", "function": {"name": "read"}},
        ]})
        # tc1 should be auto-completed, tc2 should be pending
        assert vml.pending_tool_ids == frozenset({"tc2"})
        assert len(vml) == 3
        assert vml[1]["role"] == "tool"
        assert vml[1]["tool_call_id"] == "tc1"

    def test_multiple_pending_all_auto_completed(self):
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
            {"id": "tc2", "function": {"name": "read"}},
            {"id": "tc3", "function": {"name": "write"}},
        ]})
        vml.append({"role": "user", "content": "hello"})
        assert not vml.has_pending_tools
        # assistant + 3 synthetic results + user = 5
        assert len(vml) == 5
        for i in range(1, 4):
            assert vml[i]["role"] == "tool"
            assert vml[i]["content"] == SYNTHETIC_TOOL_RESULT


class TestOrphanedToolResult:
    """Orphaned tool result handling."""

    def test_orphaned_result_accepted_in_permissive_mode(self):
        vml = ValidatedMessageList()
        # No pending tool calls, but we append a tool result
        vml.append({"role": "tool", "tool_call_id": "tc_orphan", "content": "data"})
        assert len(vml) == 1
        assert vml[0]["role"] == "tool"

    def test_orphaned_result_raises_in_strict_mode(self):
        vml = ValidatedMessageList(strict=True)
        with pytest.raises(ValueError, match="Orphaned tool result"):
            vml.append({"role": "tool", "tool_call_id": "tc_orphan", "content": "data"})

    def test_orphaned_via_add_tool_result_strict(self):
        vml = ValidatedMessageList(strict=True)
        with pytest.raises(ValueError, match="Orphaned tool result"):
            vml.add_tool_result("tc_orphan", "data")


class TestValidatedAPI:
    """Test the explicit validated methods."""

    def test_add_user(self):
        vml = ValidatedMessageList()
        vml.add_user("hello")
        assert len(vml) == 1
        assert vml[0] == {"role": "user", "content": "hello"}

    def test_add_assistant_with_tool_calls(self):
        vml = ValidatedMessageList()
        vml.add_assistant("", tool_calls=[
            {"id": "tc1", "function": {"name": "bash"}},
        ])
        assert vml.has_pending_tools
        assert len(vml) == 1

    def test_add_assistant_without_tool_calls(self):
        vml = ValidatedMessageList()
        vml.add_assistant("Hello world")
        assert not vml.has_pending_tools
        assert vml[0]["content"] == "Hello world"

    def test_add_tool_result_valid(self):
        vml = ValidatedMessageList()
        vml.add_assistant("", tool_calls=[
            {"id": "tc1", "function": {"name": "bash"}},
        ])
        vml.add_tool_result("tc1", "output here")
        assert not vml.has_pending_tools
        assert vml[1]["content"] == "output here"

    def test_add_tool_results_batch_fills_missing(self):
        vml = ValidatedMessageList()
        tool_calls = [
            {"id": "tc1", "function": {"name": "bash"}},
            {"id": "tc2", "function": {"name": "read"}},
        ]
        vml.add_assistant("", tool_calls=tool_calls)
        # Only provide result for tc1
        vml.add_tool_results_batch(tool_calls, {"tc1": "ok"})
        assert not vml.has_pending_tools
        assert len(vml) == 3
        assert vml[1]["content"] == "ok"
        assert vml[2]["content"] == SYNTHETIC_TOOL_RESULT


class TestInitialLoad:
    """Test initialization from existing messages."""

    def test_load_clean_sequence(self):
        messages = [
            {"role": "system", "content": "You are helpful"},
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "", "tool_calls": [
                {"id": "tc1", "function": {"name": "bash"}},
            ]},
            {"role": "tool", "tool_call_id": "tc1", "content": "ok"},
            {"role": "assistant", "content": "Done"},
        ]
        vml = ValidatedMessageList(messages)
        assert len(vml) == 5
        assert not vml.has_pending_tools

    def test_load_with_pending_tool_calls(self):
        messages = [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "", "tool_calls": [
                {"id": "tc1", "function": {"name": "bash"}},
            ]},
        ]
        vml = ValidatedMessageList(messages)
        assert vml.has_pending_tools
        assert vml.pending_tool_ids == frozenset({"tc1"})

    def test_load_empty(self):
        vml = ValidatedMessageList([])
        assert len(vml) == 0
        assert not vml.has_pending_tools

    def test_load_none(self):
        vml = ValidatedMessageList(None)
        assert len(vml) == 0


class TestCompaction:
    """Test slice assignment (compaction path)."""

    def test_slice_assignment_rebuilds_state(self):
        vml = ValidatedMessageList()
        vml.append({"role": "user", "content": "hello"})
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
        ]})
        # Before compaction, tc1 is pending
        assert vml.has_pending_tools

        # Simulate compaction that removes the tool call message
        compacted = [{"role": "user", "content": "[CONVERSATION SUMMARY] ..."}]
        vml[:] = compacted
        assert len(vml) == 1
        assert not vml.has_pending_tools

    def test_slice_assignment_with_new_pending(self):
        vml = ValidatedMessageList()
        # Compaction produces a message with pending tool_calls
        compacted = [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "", "tool_calls": [
                {"id": "tc_new", "function": {"name": "bash"}},
            ]},
        ]
        vml[:] = compacted
        assert vml.has_pending_tools
        assert vml.pending_tool_ids == frozenset({"tc_new"})


class TestExtendAndInsert:
    """Test extend and insert interception."""

    def test_extend_validates_each_message(self):
        vml = ValidatedMessageList()
        vml.extend([
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"},
        ])
        assert len(vml) == 2
        assert not vml.has_pending_tools

    def test_insert_rebuilds_state(self):
        vml = ValidatedMessageList()
        vml.append({"role": "user", "content": "hello"})
        vml.insert(0, {"role": "system", "content": "You are helpful"})
        assert len(vml) == 2
        assert vml[0]["role"] == "system"


class TestSystemMessages:
    """System messages pass through without affecting state."""

    def test_system_message_appends(self):
        vml = ValidatedMessageList()
        vml.append({"role": "system", "content": "You are helpful"})
        assert len(vml) == 1
        assert not vml.has_pending_tools

    def test_system_message_does_not_auto_complete(self):
        # System messages should not trigger auto-completion of pending tools
        # because they are typically prepended at the start
        vml = ValidatedMessageList()
        vml.append({"role": "assistant", "content": "", "tool_calls": [
            {"id": "tc1", "function": {"name": "bash"}},
        ]})
        # System messages are a special case — they shouldn't auto-complete
        # But since they go through the else branch (unknown/system), they don't
        assert vml.has_pending_tools


class TestThreadSafety:
    """Concurrent access does not corrupt state."""

    def test_concurrent_appends_no_crash(self):
        vml = ValidatedMessageList()
        errors = []

        def worker(thread_id):
            try:
                for i in range(50):
                    tc_id = f"t{thread_id}_tc{i}"
                    vml.append({"role": "assistant", "content": "", "tool_calls": [
                        {"id": tc_id, "function": {"name": "bash"}},
                    ]})
                    vml.append({"role": "tool", "tool_call_id": tc_id, "content": "ok"})
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=worker, args=(i,)) for i in range(4)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=10)

        assert len(errors) == 0, f"Thread safety errors: {errors}"
        # Each thread adds 50 pairs (assistant + tool) = 100 messages per thread
        assert len(vml) == 400

    def test_concurrent_auto_complete(self):
        """Multiple threads triggering auto-complete don't corrupt state."""
        vml = ValidatedMessageList()
        errors = []

        def worker(thread_id):
            try:
                for i in range(20):
                    tc_id = f"t{thread_id}_tc{i}"
                    vml.append({"role": "assistant", "content": "", "tool_calls": [
                        {"id": tc_id, "function": {"name": "bash"}},
                    ]})
                    # Don't add tool result — let auto-complete handle it
                    vml.append({"role": "user", "content": f"msg{i}"})
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=worker, args=(i,)) for i in range(4)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=10)

        assert len(errors) == 0, f"Thread safety errors: {errors}"
        assert not vml.has_pending_tools


class TestListAPI:
    """Ensure standard list operations work correctly."""

    def test_len(self):
        vml = ValidatedMessageList([
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"},
        ])
        assert len(vml) == 2

    def test_indexing(self):
        vml = ValidatedMessageList([
            {"role": "user", "content": "hello"},
        ])
        assert vml[0]["content"] == "hello"

    def test_iteration(self):
        msgs = [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"},
        ]
        vml = ValidatedMessageList(msgs)
        result = [m["role"] for m in vml]
        assert result == ["user", "assistant"]

    def test_bool(self):
        assert not ValidatedMessageList()
        assert ValidatedMessageList([{"role": "user", "content": "hello"}])
