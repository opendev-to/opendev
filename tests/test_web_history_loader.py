"""Tests for Web UI history loader fixes: thinking traces, nested tool calls, and persistence."""

import pytest
from datetime import datetime

from opendev.models.message import ChatMessage, Role, ToolCall
from opendev.web.routes.chat import ToolCallInfo, MessageResponse, tool_call_to_info


class TestToolCallToInfo:
    """Test the tool_call_to_info recursive converter."""

    def test_simple_tool_call(self):
        tc = ToolCall(
            id="tc_1",
            name="read_file",
            parameters={"path": "/tmp/test.py"},
            result="file contents here",
            result_summary="Read 10 lines",
            approved=True,
        )
        info = tool_call_to_info(tc)
        assert info.id == "tc_1"
        assert info.name == "read_file"
        assert info.parameters == {"path": "/tmp/test.py"}
        assert info.result == "file contents here"
        assert info.result_summary == "Read 10 lines"
        assert info.approved is True
        assert info.nested_tool_calls is None

    def test_nested_tool_calls_one_level(self):
        nested = ToolCall(
            id="nested_0",
            name="bash",
            parameters={"command": "ls"},
            result={"output": "file1.py\nfile2.py", "success": True},
        )
        parent = ToolCall(
            id="tc_2",
            name="spawn_subagent",
            parameters={"agent_type": "code_explorer"},
            result={"output": "Found files", "success": True},
            result_summary="Subagent completed",
            approved=True,
            nested_tool_calls=[nested],
        )
        info = tool_call_to_info(parent)
        assert info.nested_tool_calls is not None
        assert len(info.nested_tool_calls) == 1
        assert info.nested_tool_calls[0].name == "bash"
        assert info.nested_tool_calls[0].id == "nested_0"

    def test_nested_tool_calls_two_levels(self):
        deep_nested = ToolCall(
            id="nested_1",
            name="write_file",
            parameters={"path": "/tmp/out.txt"},
            result="written",
        )
        mid_nested = ToolCall(
            id="nested_0",
            name="spawn_subagent",
            parameters={"agent_type": "writer"},
            result={"output": "done", "success": True},
            nested_tool_calls=[deep_nested],
        )
        parent = ToolCall(
            id="tc_3",
            name="spawn_subagent",
            parameters={"agent_type": "orchestrator"},
            result={"output": "all done", "success": True},
            nested_tool_calls=[mid_nested],
        )
        info = tool_call_to_info(parent)
        assert info.nested_tool_calls is not None
        assert len(info.nested_tool_calls) == 1
        mid = info.nested_tool_calls[0]
        assert mid.name == "spawn_subagent"
        assert mid.nested_tool_calls is not None
        assert len(mid.nested_tool_calls) == 1
        assert mid.nested_tool_calls[0].name == "write_file"

    def test_empty_nested_returns_none(self):
        tc = ToolCall(
            id="tc_4",
            name="read_file",
            parameters={"path": "/tmp/x"},
            result="ok",
            nested_tool_calls=[],
        )
        info = tool_call_to_info(tc)
        assert info.nested_tool_calls is None


class TestMessageResponse:
    """Test MessageResponse includes thinking fields."""

    def test_thinking_trace_included(self):
        resp = MessageResponse(
            role="assistant",
            content="Hello",
            thinking_trace="I should greet the user",
            reasoning_content=None,
        )
        assert resp.thinking_trace == "I should greet the user"
        assert resp.reasoning_content is None

    def test_reasoning_content_included(self):
        resp = MessageResponse(
            role="assistant",
            content="Result",
            thinking_trace=None,
            reasoning_content="Step 1: analyze\nStep 2: respond",
        )
        assert resp.reasoning_content == "Step 1: analyze\nStep 2: respond"

    def test_both_thinking_fields(self):
        resp = MessageResponse(
            role="assistant",
            content="Answer",
            thinking_trace="thinking...",
            reasoning_content="reasoning...",
        )
        assert resp.thinking_trace == "thinking..."
        assert resp.reasoning_content == "reasoning..."


class TestToolCallInfoSelfRef:
    """Test ToolCallInfo self-referential model validation."""

    def test_self_referential_validation(self):
        info = ToolCallInfo(
            id="outer",
            name="spawn_subagent",
            parameters={},
            nested_tool_calls=[
                ToolCallInfo(
                    id="inner",
                    name="bash",
                    parameters={"cmd": "ls"},
                    result="output",
                )
            ],
        )
        assert info.nested_tool_calls is not None
        assert len(info.nested_tool_calls) == 1
        assert info.nested_tool_calls[0].id == "inner"

    def test_json_roundtrip(self):
        info = ToolCallInfo(
            id="outer",
            name="spawn_subagent",
            parameters={"type": "explorer"},
            nested_tool_calls=[
                ToolCallInfo(
                    id="n0",
                    name="read_file",
                    parameters={"path": "/tmp/x"},
                    result="contents",
                    nested_tool_calls=None,
                )
            ],
        )
        json_str = info.model_dump_json()
        restored = ToolCallInfo.model_validate_json(json_str)
        assert restored.nested_tool_calls is not None
        assert restored.nested_tool_calls[0].name == "read_file"


class TestWebUICallbackNestedCalls:
    """Test WebUICallback nested call collection and clearing."""

    def test_get_and_clear_nested_calls(self):
        from unittest.mock import MagicMock, AsyncMock
        import asyncio

        from opendev.web.web_ui_callback import WebUICallback

        loop = asyncio.new_event_loop()
        ws_manager = MagicMock()
        ws_manager.broadcast = AsyncMock()
        state = MagicMock()

        cb = WebUICallback(
            ws_manager=ws_manager,
            loop=loop,
            session_id="test-session",
            state=state,
        )

        # Initially empty
        assert cb.get_and_clear_nested_calls() == []

        # Simulate nested tool results (bypass broadcast by mocking)
        cb._pending_nested_calls.append(
            ToolCall(id="nested_0", name="bash", parameters={"cmd": "ls"}, result="files")
        )
        cb._pending_nested_calls.append(
            ToolCall(id="nested_1", name="read_file", parameters={"path": "/x"}, result="data")
        )

        calls = cb.get_and_clear_nested_calls()
        assert len(calls) == 2
        assert calls[0].name == "bash"
        assert calls[1].name == "read_file"

        # Buffer should be cleared
        assert cb.get_and_clear_nested_calls() == []

        loop.close()


class TestReconstructAndPersistMessages:
    """Tests for the old _reconstruct_and_persist_messages method.

    NOTE: This method was removed when Web UI was unified to use ReactExecutor,
    which handles step-by-step persistence via SessionPersistenceMixin._persist_step().
    These tests are kept as placeholders to document the migration.
    """

    pass
