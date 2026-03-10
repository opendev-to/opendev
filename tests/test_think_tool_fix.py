"""Test cases for the separate thinking phase architecture.

These tests verify that:
1. Thinking is now a pre-processing phase, not a tool
2. _get_thinking_trace() makes a separate LLM call for reasoning
3. Thinking trace is injected as a user message before action phase
4. Regular tools still work correctly
"""

import json
import os
import sys
from unittest.mock import MagicMock, patch

import pytest

# Add parent dir to path for imports
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


class TestThinkingPhaseArchitecture:
    """Test the separate thinking phase approach."""

    def test_get_thinking_trace_makes_separate_call(self):
        """Verify _get_thinking_trace makes a separate LLM call without tools."""
        from opendev.repl.react_executor import ReactExecutor

        # Create a minimal executor with proper session manager mock
        mock_session_manager = MagicMock()
        mock_session_manager.current_session = None

        executor = ReactExecutor(
            session_manager=mock_session_manager,
            config=MagicMock(),
            mode_manager=MagicMock(),
            console=MagicMock(),
            llm_caller=MagicMock(),
            tool_executor=MagicMock(),
        )

        # Mock the agent - template must contain {context} placeholder
        mock_agent = MagicMock()
        mock_agent.build_system_prompt.return_value = "Thinking system prompt\n{context}"
        mock_agent.call_thinking_llm.return_value = {
            "success": True,
            "content": "Analysis: The user wants to list files. Next step: call list_files."
        }

        messages = [
            {"role": "system", "content": "Main system prompt"},
            {"role": "user", "content": "List files in this directory"},
        ]

        # Mock UI callback
        mock_ui_callback = MagicMock()
        mock_ui_callback.on_thinking = MagicMock()

        result = executor._get_thinking_trace(messages, mock_agent, mock_ui_callback)

        # Should call build_system_prompt with thinking_visible=True
        mock_agent.build_system_prompt.assert_called_once_with(thinking_visible=True)

        # Should call call_thinking_llm with dual-memory messages
        mock_agent.call_thinking_llm.assert_called_once()
        call_args = mock_agent.call_thinking_llm.call_args[0][0]

        # First message should be thinking system prompt (with context injected)
        assert call_args[0]["role"] == "system"
        assert "Thinking system prompt" in call_args[0]["content"]

        # Second message should be the thinking analysis prompt (not the original user message)
        assert call_args[1]["role"] == "user"

        # Should display thinking via UI callback
        mock_ui_callback.on_thinking.assert_called_once()

        # Should return the thinking trace
        assert "Analysis" in result
        assert "list_files" in result

    def test_get_thinking_trace_returns_none_on_failure(self):
        """Verify _get_thinking_trace returns None when LLM call fails."""
        from opendev.repl.react_executor import ReactExecutor

        mock_session_manager = MagicMock()
        mock_session_manager.current_session = None

        executor = ReactExecutor(
            session_manager=mock_session_manager,
            config=MagicMock(),
            mode_manager=MagicMock(),
            console=MagicMock(),
            llm_caller=MagicMock(),
            tool_executor=MagicMock(),
        )

        mock_agent = MagicMock()
        mock_agent.build_system_prompt.return_value = "Thinking system prompt\n{context}"
        mock_agent.call_thinking_llm.return_value = {
            "success": False,
            "error": "API Error",
            "content": ""
        }

        result = executor._get_thinking_trace([], mock_agent, None)

        assert result is None

    def test_regular_tool_adds_tool_message(self):
        """Verify regular tools still add tool role messages."""
        from opendev.repl.react_executor import ReactExecutor

        mock_session_manager = MagicMock()
        mock_session_manager.current_session = None

        executor = ReactExecutor(
            session_manager=mock_session_manager,
            config=MagicMock(),
            mode_manager=MagicMock(),
            console=MagicMock(),
            llm_caller=MagicMock(),
            tool_executor=MagicMock(),
        )

        messages = []
        tool_call = {
            "id": "call_456",
            "function": {
                "name": "read_file",
                "arguments": '{"path": "/tmp/test.txt"}',
            },
        }
        result = {"success": True, "output": "file contents here"}

        executor._add_tool_result_to_history(messages, tool_call, result)

        assert len(messages) == 1
        assert messages[0]["role"] == "tool"
        assert messages[0]["tool_call_id"] == "call_456"
        assert messages[0]["content"] == "file contents here"


class TestThinkingSystemPrompt:
    """Test thinking system prompt has correct guidance."""

    def test_prompt_describes_thinking_phase(self):
        """Verify system prompt explains the reasoning/analysis phase."""
        prompt_path = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "swecli/core/agents/prompts/templates/system/thinking_system_prompt.txt"
        )

        with open(prompt_path, "r") as f:
            content = f.read()

        # Prompt should describe reasoning/analysis responsibilities
        assert "reasoning" in content.lower() or "analyze" in content.lower(), \
            "Prompt should mention reasoning or analysis"
        # Should have context placeholder for dual memory injection
        assert "{context}" in content, \
            "Prompt should have {context} placeholder"

    def test_prompt_has_no_think_tool_reference(self):
        """Verify system prompt does not reference think tool."""
        prompt_path = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "swecli/core/agents/prompts/templates/system/thinking_system_prompt.txt"
        )

        with open(prompt_path, "r") as f:
            content = f.read()

        # Should not reference think tool
        assert "think tool" not in content.lower(), \
            "Prompt should not reference think tool"
        assert "When to Use Think Tool" not in content, \
            "Prompt should not have 'When to Use Think Tool' section"


class TestCallThinkingLLMSignature:
    """Test the call_thinking_llm method signature and behavior.

    Note: Full integration tests with MainAgent are complex due to
    initialization requirements. These tests verify the method contract.
    """

    def test_call_thinking_llm_method_exists(self):
        """Verify call_thinking_llm method exists on MainAgent."""
        from opendev.core.agents.main_agent import MainAgent
        import inspect

        assert hasattr(MainAgent, "call_thinking_llm"), \
            "MainAgent should have call_thinking_llm method"

        # Check the signature
        sig = inspect.signature(MainAgent.call_thinking_llm)
        param_names = list(sig.parameters.keys())

        # Should have self, messages, and optionally task_monitor
        assert "self" in param_names
        assert "messages" in param_names
        # Should NOT have tools-related parameters
        assert "tools" not in param_names
        assert "tool_choice" not in param_names
        assert "thinking_visible" not in param_names

    def test_call_llm_method_no_force_think(self):
        """Verify call_llm no longer accepts force_think parameter."""
        from opendev.core.agents.main_agent import MainAgent
        import inspect

        sig = inspect.signature(MainAgent.call_llm)
        param_names = list(sig.parameters.keys())

        assert "force_think" not in param_names, \
            "call_llm should not have force_think parameter anymore"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
