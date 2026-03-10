"""Tests for message schema validation."""

import pytest

from opendev.models.message import ChatMessage, Role, ToolCall
from opendev.models.message_validator import (
    ValidationVerdict,
    filter_and_repair_messages,
    repair_message,
    validate_message,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _user(content="Hello"):
    return ChatMessage(role=Role.USER, content=content)


def _assistant(content="Sure", tool_calls=None, **kwargs):
    return ChatMessage(role=Role.ASSISTANT, content=content, tool_calls=tool_calls or [], **kwargs)


def _system(content="You are helpful"):
    return ChatMessage(role=Role.SYSTEM, content=content)


def _tool_call(name="read_file", result="ok", error=None, parameters=None, nested=None, tc_id=None):
    return ToolCall(
        id=tc_id or f"tc_{name}",
        name=name,
        parameters=parameters if parameters is not None else {"path": "/tmp/f"},
        result=result,
        error=error,
        nested_tool_calls=nested or [],
    )


# ===========================================================================
# Basic validation tests
# ===========================================================================

class TestBasicValidation:
    def test_valid_user_message(self):
        assert validate_message(_user("Hi")).is_valid

    def test_valid_assistant_message(self):
        assert validate_message(_assistant("Sure")).is_valid

    def test_valid_system_message(self):
        assert validate_message(_system("You are helpful")).is_valid

    def test_empty_user_rejected(self):
        v = validate_message(_user(""))
        assert not v.is_valid
        assert "empty content" in v.reason

    def test_whitespace_user_rejected(self):
        assert not validate_message(_user("   ")).is_valid

    def test_empty_system_rejected(self):
        assert not validate_message(_system("")).is_valid

    def test_empty_assistant_no_content_no_tools_rejected(self):
        msg = ChatMessage(role=Role.ASSISTANT, content="", tool_calls=[])
        v = validate_message(msg)
        assert not v.is_valid
        assert "no content and no tool_calls" in v.reason

    def test_assistant_content_only_passes(self):
        assert validate_message(_assistant("Hello", tool_calls=[])).is_valid

    def test_assistant_tools_only_passes(self):
        msg = _assistant(content="", tool_calls=[_tool_call()])
        assert validate_message(msg).is_valid

    def test_user_with_tool_calls_rejected(self):
        msg = ChatMessage(role=Role.USER, content="Hi", tool_calls=[_tool_call()])
        v = validate_message(msg)
        assert not v.is_valid
        assert "tool_calls" in v.reason


# ===========================================================================
# Tool call validation tests
# ===========================================================================

class TestToolCallValidation:
    def test_tool_call_with_result_passes(self):
        msg = _assistant(content="", tool_calls=[_tool_call(result="done")])
        assert validate_message(msg).is_valid

    def test_tool_call_with_error_passes(self):
        msg = _assistant(content="", tool_calls=[_tool_call(result=None, error="fail")])
        assert validate_message(msg).is_valid

    def test_tool_call_no_result_no_error_rejected(self):
        msg = _assistant(content="", tool_calls=[_tool_call(result=None, error=None)])
        v = validate_message(msg)
        assert not v.is_valid
        assert "no result and no error" in v.reason

    def test_task_complete_no_result_passes(self):
        tc = _tool_call(name="task_complete", result=None, error=None)
        msg = _assistant(content="", tool_calls=[tc])
        assert validate_message(msg).is_valid

    def test_empty_id_rejected(self):
        tc = ToolCall(id="", name="read", parameters={}, result="ok")
        msg = _assistant(content="", tool_calls=[tc])
        v = validate_message(msg)
        assert not v.is_valid
        assert "empty id" in v.reason

    def test_empty_name_rejected(self):
        tc = ToolCall(id="tc1", name="", parameters={}, result="ok")
        msg = _assistant(content="", tool_calls=[tc])
        v = validate_message(msg)
        assert not v.is_valid
        assert "empty name" in v.reason

    def test_non_dict_parameters_rejected(self):
        # Use model_construct to bypass Pydantic type enforcement
        tc = ToolCall.model_construct(id="tc1", name="read", parameters="bad", result="ok",
                                       nested_tool_calls=[])
        msg = ChatMessage.model_construct(role=Role.ASSISTANT, content="", tool_calls=[tc],
                                           metadata={})
        v = validate_message(msg)
        assert not v.is_valid
        assert "non-dict parameters" in v.reason

    def test_non_serializable_result_rejected(self):
        tc = ToolCall.model_construct(id="tc1", name="read", parameters={}, result=lambda: None,
                                       error=None, nested_tool_calls=[])
        msg = ChatMessage.model_construct(role=Role.ASSISTANT, content="", tool_calls=[tc],
                                           metadata={})
        v = validate_message(msg)
        assert not v.is_valid
        assert "non-serializable result" in v.reason


# ===========================================================================
# Nested tool call tests
# ===========================================================================

class TestNestedToolCalls:
    def test_valid_nested_passes(self):
        inner = _tool_call(name="inner", result="ok")
        outer = _tool_call(name="outer", result="ok", nested=[inner])
        msg = _assistant(content="", tool_calls=[outer])
        assert validate_message(msg).is_valid

    def test_nested_no_result_no_error_rejected(self):
        inner = _tool_call(name="inner", result=None, error=None)
        outer = _tool_call(name="outer", result="ok", nested=[inner])
        msg = _assistant(content="", tool_calls=[outer])
        v = validate_message(msg)
        assert not v.is_valid
        assert "no result and no error" in v.reason


# ===========================================================================
# Serialization tests
# ===========================================================================

class TestSerializationValidation:
    def test_non_serializable_token_usage_rejected(self):
        msg = ChatMessage.model_construct(
            role=Role.ASSISTANT, content="hi", tool_calls=[], metadata={},
            token_usage={"bad": lambda: None}, thinking_trace=None, reasoning_content=None,
        )
        v = validate_message(msg)
        assert not v.is_valid
        assert "token_usage" in v.reason

    def test_non_dict_token_usage_rejected(self):
        msg = ChatMessage.model_construct(
            role=Role.ASSISTANT, content="hi", tool_calls=[], metadata={},
            token_usage="not a dict", thinking_trace=None, reasoning_content=None,
        )
        v = validate_message(msg)
        assert not v.is_valid
        assert "token_usage" in v.reason

    def test_non_serializable_metadata_rejected(self):
        msg = ChatMessage.model_construct(
            role=Role.ASSISTANT, content="hi", tool_calls=[],
            metadata={"fn": lambda: None}, token_usage=None,
            thinking_trace=None, reasoning_content=None,
        )
        v = validate_message(msg)
        assert not v.is_valid
        assert "metadata" in v.reason

    def test_empty_thinking_trace_rejected(self):
        msg = _assistant("hi", thinking_trace="")
        v = validate_message(msg)
        assert not v.is_valid
        assert "thinking_trace" in v.reason

    def test_empty_reasoning_content_rejected(self):
        msg = _assistant("hi", reasoning_content="  ")
        v = validate_message(msg)
        assert not v.is_valid
        assert "reasoning_content" in v.reason

    def test_valid_thinking_trace_passes(self):
        msg = _assistant("hi", thinking_trace="I thought about it")
        assert validate_message(msg).is_valid


# ===========================================================================
# Repair tests
# ===========================================================================

class TestRepairMessage:
    def test_drops_empty_message(self):
        msg = ChatMessage(role=Role.ASSISTANT, content="", tool_calls=[])
        assert repair_message(msg) is None

    def test_fixes_incomplete_tool_call(self):
        tc = _tool_call(result=None, error=None)
        msg = _assistant(content="", tool_calls=[tc])
        fixed = repair_message(msg)
        assert fixed is not None
        assert fixed.tool_calls[0].error == "Tool execution was interrupted or never completed."

    def test_coerces_non_serializable_result(self):
        tc = ToolCall.model_construct(id="tc1", name="read", parameters={},
                                       result=lambda: None, error=None, nested_tool_calls=[])
        msg = ChatMessage.model_construct(role=Role.ASSISTANT, content="done", tool_calls=[tc],
                                           metadata={}, token_usage=None,
                                           thinking_trace=None, reasoning_content=None)
        fixed = repair_message(msg)
        assert fixed is not None
        assert isinstance(fixed.tool_calls[0].result, str)

    def test_normalizes_empty_thinking_trace(self):
        msg = _assistant("hi", thinking_trace="")
        fixed = repair_message(msg)
        assert fixed is not None
        assert fixed.thinking_trace is None

    def test_normalizes_empty_reasoning_content(self):
        msg = _assistant("hi", reasoning_content="  ")
        fixed = repair_message(msg)
        assert fixed is not None
        assert fixed.reasoning_content is None

    def test_fixes_string_parameters(self):
        tc = ToolCall.model_construct(id="tc1", name="read", parameters='{"path": "/tmp"}',
                                       result="ok", error=None, nested_tool_calls=[])
        msg = ChatMessage.model_construct(role=Role.ASSISTANT, content="done", tool_calls=[tc],
                                           metadata={}, token_usage=None,
                                           thinking_trace=None, reasoning_content=None)
        fixed = repair_message(msg)
        assert fixed is not None
        assert fixed.tool_calls[0].parameters == {"path": "/tmp"}

    def test_fixes_unparseable_string_parameters(self):
        tc = ToolCall.model_construct(id="tc1", name="read", parameters="not json",
                                       result="ok", error=None, nested_tool_calls=[])
        msg = ChatMessage.model_construct(role=Role.ASSISTANT, content="done", tool_calls=[tc],
                                           metadata={}, token_usage=None,
                                           thinking_trace=None, reasoning_content=None)
        fixed = repair_message(msg)
        assert fixed is not None
        assert fixed.tool_calls[0].parameters == {"raw": "not json"}

    def test_fixes_non_serializable_token_usage(self):
        msg = ChatMessage.model_construct(
            role=Role.ASSISTANT, content="hi", tool_calls=[], metadata={},
            token_usage="not a dict", thinking_trace=None, reasoning_content=None,
        )
        fixed = repair_message(msg)
        assert fixed is not None
        assert fixed.token_usage is None

    def test_coerces_non_serializable_metadata_values(self):
        msg = ChatMessage.model_construct(
            role=Role.ASSISTANT, content="hi", tool_calls=[],
            metadata={"fn": lambda: None}, token_usage=None,
            thinking_trace=None, reasoning_content=None,
        )
        fixed = repair_message(msg)
        assert fixed is not None
        assert isinstance(fixed.metadata["fn"], str)


# ===========================================================================
# Bulk filter tests
# ===========================================================================

class TestFilterAndRepairMessages:
    def test_removes_empty_keeps_valid(self):
        msgs = [
            _user("Hi"),
            ChatMessage(role=Role.ASSISTANT, content="", tool_calls=[]),  # empty → drop
            _assistant("Hello"),
        ]
        result = filter_and_repair_messages(msgs)
        assert len(result) == 2
        assert result[0].content == "Hi"
        assert result[1].content == "Hello"

    def test_repairs_incomplete_tool_calls(self):
        tc = _tool_call(result=None, error=None)
        msgs = [
            _user("Do something"),
            _assistant(content="", tool_calls=[tc]),
        ]
        result = filter_and_repair_messages(msgs)
        assert len(result) == 2
        assert result[1].tool_calls[0].error is not None

    def test_mixed_valid_invalid_repairable(self):
        msgs = [
            _user("Hi"),
            ChatMessage(role=Role.ASSISTANT, content="", tool_calls=[]),  # drop
            _assistant("Hello", thinking_trace=""),  # repair thinking_trace
            _assistant(content="", tool_calls=[_tool_call(result=None, error=None)]),  # repair tc
            _system("sys"),
        ]
        result = filter_and_repair_messages(msgs)
        assert len(result) == 4  # dropped the empty assistant
        assert result[1].thinking_trace is None  # repaired
        assert result[2].tool_calls[0].error is not None  # repaired


# ===========================================================================
# Session.add_message integration
# ===========================================================================

class TestSessionAddMessage:
    def test_valid_message_added(self):
        from opendev.models.session import Session

        session = Session()
        result = session.add_message(_user("Hello"))
        assert result is True
        assert len(session.messages) == 1

    def test_invalid_message_rejected(self):
        from opendev.models.session import Session

        session = Session()
        result = session.add_message(_user(""))
        assert result is False
        assert len(session.messages) == 0
