"""Tests for AgentCreatorController LLM generation."""

import pytest
from unittest.mock import MagicMock, AsyncMock, patch
from pathlib import Path
import tempfile


class TestParseGeneratedAgent:
    """Test the _parse_generated_agent method."""

    def setup_method(self):
        """Create a controller instance for testing."""
        from swecli.ui_textual.controllers.agent_creator_controller import (
            AgentCreatorController,
        )

        mock_app = MagicMock()
        self.controller = AgentCreatorController(mock_app)

    def test_parse_simple_agent(self):
        """Test parsing a simple agent definition."""
        content = """---
name: test-agent
description: "A test agent"
model: sonnet
---

You are a test agent.
"""
        name, parsed_content = self.controller._parse_generated_agent(content, "test")
        assert name == "test-agent"
        assert "You are a test agent." in parsed_content

    def test_parse_agent_with_code_block(self):
        """Test parsing agent wrapped in markdown code block."""
        content = """```markdown
---
name: code-reviewer
description: "Reviews code"
model: sonnet
---

You review code.
```"""
        name, parsed_content = self.controller._parse_generated_agent(content, "test")
        assert name == "code-reviewer"
        assert "You review code." in parsed_content
        assert "```" not in parsed_content

    def test_parse_agent_name_cleanup(self):
        """Test that agent names are cleaned up properly."""
        content = """---
name: "Test Agent With Spaces"
description: "A test"
model: sonnet
---

Content here.
"""
        name, parsed_content = self.controller._parse_generated_agent(content, "test")
        assert name == "test-agent-with-spaces"
        assert "-" in name
        assert " " not in name

    def test_parse_agent_no_name(self):
        """Test fallback when no name is found."""
        content = """---
description: "No name field"
model: sonnet
---

Content here.
"""
        name, parsed_content = self.controller._parse_generated_agent(content, "test")
        assert name == "custom-agent"

    def test_parse_agent_name_too_long(self):
        """Test that long names are truncated."""
        content = """---
name: this-is-a-very-very-very-very-long-agent-name-that-should-be-truncated
description: "A test"
model: sonnet
---

Content here.
"""
        name, parsed_content = self.controller._parse_generated_agent(content, "test")
        assert len(name) <= 30


class TestCreateAgentFallback:
    """Test the _create_agent_fallback method."""

    def setup_method(self):
        """Create a controller instance for testing."""
        from swecli.ui_textual.controllers.agent_creator_controller import (
            AgentCreatorController,
        )

        mock_app = MagicMock()
        mock_app.conversation = MagicMock()
        mock_app.conversation.lines = []
        mock_app.conversation.scroll_end = MagicMock()
        mock_app.conversation._truncate_from = MagicMock()
        mock_app.conversation.write = MagicMock()
        mock_app.refresh = MagicMock()

        self.controller = AgentCreatorController(mock_app)

    def test_fallback_creates_agent_file(self):
        """Test that fallback stores agent name and prompt in state."""
        import asyncio

        self.controller.state = {"panel_start": 0}
        self.controller._render_current_panel = MagicMock()

        asyncio.run(
            self.controller._create_agent_fallback("An agent that helps debug code", "Error")
        )

        # Verify agent state was set
        assert "agent_name" in self.controller.state
        assert "system_prompt" in self.controller.state
        assert "debug" in self.controller.state["system_prompt"].lower()
        assert len(self.controller.state["agent_name"]) > 0

    def test_fallback_name_extraction(self):
        """Test that fallback extracts meaningful name from description."""
        import asyncio

        self.controller.state = {"panel_start": 0}
        self.controller._render_current_panel = MagicMock()

        asyncio.run(
            self.controller._create_agent_fallback(
                "An agent for testing Python applications", "Error"
            )
        )

        # Name should contain meaningful words from description
        name = self.controller.state["agent_name"]
        assert "agent" in name or "testing" in name or "python" in name


class TestAgentGeneratorPromptExists:
    """Test that the agent generator prompt file exists and is valid."""

    def test_prompt_file_exists(self):
        """Test that the agent generator prompt file exists."""
        prompt_path = (
            Path(__file__).parent.parent
            / "swecli/core/agents/prompts/templates/generators/agent_generator_prompt.txt"
        )
        assert prompt_path.exists(), f"Prompt file not found at {prompt_path}"

    def test_prompt_has_required_content(self):
        """Test that prompt contains key instructions."""
        prompt_path = (
            Path(__file__).parent.parent
            / "swecli/core/agents/prompts/templates/generators/agent_generator_prompt.txt"
        )
        content = prompt_path.read_text()

        # Check for key elements that should be in the prompt
        assert "name:" in content.lower() or "kebab-case" in content.lower()
        assert "description" in content.lower()
        assert "system prompt" in content.lower() or "systemprompt" in content.lower()
        assert "---" in content  # YAML frontmatter instruction
