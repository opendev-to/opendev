"""Integration tests for thinking mode architecture.

These tests verify that the thinking mode architecture is correctly set up:
- Think tool is removed from schemas (new architecture)
- Thinking is a separate pre-processing phase via _get_thinking_trace()
- call_llm does not include think tool regardless of thinking_visible

Run with: pytest tests/test_thinking_mode_integration.py -v
"""

import os
import pytest
from unittest.mock import MagicMock, patch

from swecli.core.agents.main_agent import MainAgent
from swecli.models.config import AppConfig


def create_test_agent() -> MainAgent:
    """Create a minimal agent for testing with patched init."""
    config = AppConfig()
    config.model_provider = "openai"
    config.model = "gpt-4o-mini"

    tool_registry = MagicMock()
    tool_registry.subagent_manager = None
    tool_registry.get_all_mcp_tools.return_value = []

    mode_manager = MagicMock()

    with patch.object(MainAgent, 'build_system_prompt', return_value="Test prompt"), \
         patch.object(MainAgent, 'build_tool_schemas', return_value=[]):
        agent = MainAgent(config, tool_registry, mode_manager)

    return agent


class TestThinkingModeDisabled:
    """Test that think tool is properly removed from the architecture."""

    def test_think_tool_removed_from_schemas(self):
        """Test that think tool is not in schemas (removed in new architecture)."""
        agent = create_test_agent()

        schemas_on = agent.build_tool_schemas(thinking_visible=True)
        schemas_off = agent.build_tool_schemas(thinking_visible=False)

        names_on = [s["function"]["name"] for s in schemas_on]
        names_off = [s["function"]["name"] for s in schemas_off]

        assert "think" not in names_on, "think tool should be removed from schemas"
        assert "think" not in names_off, "think tool should be removed from schemas"


class TestThinkingPhaseArchitecture:
    """Test the new separate thinking phase architecture."""

    def test_thinking_is_separate_phase(self):
        """Verify thinking is a separate pre-processing phase, not a tool."""
        agent = create_test_agent()

        # Think tool should NOT be in any schema
        schemas = agent.build_tool_schemas(thinking_visible=True)
        names = [s["function"]["name"] for s in schemas]
        assert "think" not in names

    def test_call_thinking_llm_exists(self):
        """Verify call_thinking_llm method exists for the separate phase."""
        assert hasattr(MainAgent, "call_thinking_llm"), \
            "MainAgent should have call_thinking_llm for the separate thinking phase"

    def test_build_system_prompt_accepts_thinking_visible(self):
        """Verify build_system_prompt accepts thinking_visible parameter."""
        import inspect
        sig = inspect.signature(MainAgent.build_system_prompt)
        assert "thinking_visible" in sig.parameters
