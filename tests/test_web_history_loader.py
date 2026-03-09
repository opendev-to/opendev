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
    """Test AgentExecutor._reconstruct_and_persist_messages."""

    def _make_executor(self):
        from unittest.mock import MagicMock

        state = MagicMock()
        from opendev.web.agent_executor import AgentExecutor

        executor = AgentExecutor(state)
        return executor

    def _make_session(self):
        from unittest.mock import MagicMock

        session = MagicMock()
        session.messages = []

        def add_message(msg):
            session.messages.append(msg)

        session.add_message = add_message
        return session

    def test_multi_step_conversation(self):
        """2 tool calls + final response produces 3 ChatMessages."""
        import json

        executor = self._make_executor()
        session = self._make_session()

        result = {
            "content": "Done!",
            "messages": [
                {"role": "system", "content": "system prompt"},
                {"role": "user", "content": "read foo and bar"},
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "tc_1",
                            "function": {
                                "name": "read_file",
                                "arguments": json.dumps({"path": "/tmp/foo.py"}),
                            },
                        }
                    ],
                },
                {"role": "tool", "tool_call_id": "tc_1", "content": "foo contents"},
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "tc_2",
                            "function": {
                                "name": "read_file",
                                "arguments": json.dumps({"path": "/tmp/bar.py"}),
                            },
                        }
                    ],
                },
                {"role": "tool", "tool_call_id": "tc_2", "content": "bar contents"},
                {"role": "assistant", "content": "Done!"},
            ],
        }

        executor._reconstruct_and_persist_messages(
            session, result, "thinking...", None, {"total": 100}, None
        )

        assert len(session.messages) == 3
        # First assistant msg has tool calls and thinking
        assert len(session.messages[0].tool_calls) == 1
        assert session.messages[0].tool_calls[0].name == "read_file"
        assert session.messages[0].tool_calls[0].result == "foo contents"
        assert session.messages[0].thinking_trace == "thinking..."
        # Second assistant msg has tool calls but no thinking
        assert len(session.messages[1].tool_calls) == 1
        assert session.messages[1].thinking_trace is None
        # Final assistant msg has content, no tool calls
        assert session.messages[2].content == "Done!"
        assert len(session.messages[2].tool_calls) == 0

    def test_tool_results_matched_by_id(self):
        """Tool results are correctly matched by tool_call_id."""
        import json

        executor = self._make_executor()
        session = self._make_session()

        result = {
            "content": "ok",
            "messages": [
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "tc_a",
                            "function": {
                                "name": "bash",
                                "arguments": json.dumps({"command": "ls"}),
                            },
                        },
                        {
                            "id": "tc_b",
                            "function": {
                                "name": "read_file",
                                "arguments": json.dumps({"path": "/x"}),
                            },
                        },
                    ],
                },
                {"role": "tool", "tool_call_id": "tc_a", "content": "file1\nfile2"},
                {"role": "tool", "tool_call_id": "tc_b", "content": "file content"},
                {"role": "assistant", "content": "ok"},
            ],
        }

        executor._reconstruct_and_persist_messages(
            session, result, None, None, None, None
        )

        assert len(session.messages) == 2
        tc_a = session.messages[0].tool_calls[0]
        tc_b = session.messages[0].tool_calls[1]
        assert tc_a.result == "file1\nfile2"
        assert tc_b.result == "file content"

    def test_error_detection(self):
        """Tool results starting with 'Error' are detected as errors."""
        import json

        executor = self._make_executor()
        session = self._make_session()

        result = {
            "content": "failed",
            "messages": [
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "tc_err",
                            "function": {
                                "name": "bash",
                                "arguments": json.dumps({"command": "bad"}),
                            },
                        }
                    ],
                },
                {
                    "role": "tool",
                    "tool_call_id": "tc_err",
                    "content": "Error in bash: command not found",
                },
                {"role": "assistant", "content": "failed"},
            ],
        }

        executor._reconstruct_and_persist_messages(
            session, result, None, None, None, None
        )

        tc = session.messages[0].tool_calls[0]
        assert tc.error == "Error in bash: command not found"
        assert tc.result == "Error in bash: command not found"

    def test_thinking_trace_first_assistant_only(self):
        """thinking_trace is attached to first assistant message only."""
        import json

        executor = self._make_executor()
        session = self._make_session()

        result = {
            "content": "done",
            "messages": [
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "tc_1",
                            "function": {
                                "name": "bash",
                                "arguments": json.dumps({"command": "echo hi"}),
                            },
                        }
                    ],
                },
                {"role": "tool", "tool_call_id": "tc_1", "content": "hi"},
                {"role": "assistant", "content": "done"},
            ],
        }

        executor._reconstruct_and_persist_messages(
            session, result, "my thinking", "my reasoning", None, None
        )

        assert session.messages[0].thinking_trace == "my thinking"
        assert session.messages[0].reasoning_content == "my reasoning"
        assert session.messages[1].thinking_trace is None
        assert session.messages[1].reasoning_content is None

    def test_fallback_no_assistant_messages(self):
        """Empty messages list falls back to content-only message."""
        executor = self._make_executor()
        session = self._make_session()

        result = {"content": "simple response", "messages": []}

        executor._reconstruct_and_persist_messages(
            session, result, "trace", None, {"total": 50}, None
        )

        assert len(session.messages) == 1
        assert session.messages[0].content == "simple response"
        assert session.messages[0].thinking_trace == "trace"
        assert session.messages[0].token_usage == {"total": 50}

    def test_no_tool_calls_single_message(self):
        """Single assistant message without tool calls produces one ChatMessage."""
        executor = self._make_executor()
        session = self._make_session()

        result = {
            "content": "hello",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "hello"},
            ],
        }

        executor._reconstruct_and_persist_messages(
            session, result, None, None, {"total": 10}, None
        )

        assert len(session.messages) == 1
        assert session.messages[0].content == "hello"
        assert session.messages[0].token_usage == {"total": 10}
