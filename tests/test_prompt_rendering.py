"""Test template rendering and variable substitution."""

import pytest
from pathlib import Path

from swecli.core.agents.prompts.renderer import PromptRenderer
from swecli.core.agents.prompts.variables import PromptVariables


def test_variable_registry():
    """Test that PromptVariables registry has all required variables."""
    variables = PromptVariables()

    # Tool variables
    assert variables.EDIT_TOOL.name == "edit_file"
    assert variables.WRITE_TOOL.name == "write_file"
    assert variables.READ_TOOL.name == "read_file"
    assert variables.BASH_TOOL.name == "run_command"
    assert variables.GLOB_TOOL.name == "list_files"
    assert variables.GREP_TOOL.name == "search"
    assert variables.PRESENT_PLAN_TOOL.name == "present_plan"
    assert variables.ASK_USER_QUESTION_TOOL_NAME == "ask_user"

    # Agent config
    assert variables.EXPLORE_AGENT_COUNT == 3
    assert variables.PLAN_AGENT_COUNT == 1


def test_system_reminder_variable():
    """Test SystemReminderVariable creation."""
    variables = PromptVariables()

    # Test with non-existent path
    reminder = variables.get_system_reminder("/tmp/nonexistent-plan.md")
    assert reminder.planFilePath == "/tmp/nonexistent-plan.md"
    assert reminder.planExists is False

    # Test with existing path
    import tempfile

    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write("Test plan")
        plan_path = f.name

    try:
        reminder = variables.get_system_reminder(plan_path)
        assert reminder.planFilePath == plan_path
        assert reminder.planExists is True
    finally:
        Path(plan_path).unlink()


def test_template_rendering_basic():
    """Test basic template rendering with variable substitution."""
    renderer = PromptRenderer()

    # Create a simple test template
    import tempfile

    template_content = """<!--
name: 'Test Template'
version: 1.0.0
-->

This is a test template.
Present tool: ${PRESENT_PLAN_TOOL.name}
Ask user tool: ${ASK_USER_QUESTION_TOOL_NAME}
Explore count: ${EXPLORE_AGENT_COUNT}
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write(template_content)
        template_path = Path(f.name)

    try:
        result = renderer.render(template_path)

        # YAML frontmatter should be stripped
        assert "<!--" not in result
        assert "name: 'Test Template'" not in result

        # Variables should be substituted
        assert "Present tool: present_plan" in result
        assert "Ask user tool: ask_user" in result
        assert "Explore count: 3" in result
    finally:
        template_path.unlink()


def test_tool_description_loading(tmp_path):
    """Test that tool descriptions load from markdown files via loader."""
    from swecli.core.agents.prompts.loader import load_tool_description
    from swecli.core.context_engineering.tools.implementations.present_plan_tool import (
        PresentPlanTool,
    )

    # Tool name is correct
    tool = PresentPlanTool()
    assert tool.name == "present_plan"

    # Description loads via load_tool_description (used by definitions.py)
    desc = load_tool_description("present_plan")
    assert "How This Tool Works" in desc
    assert "plan" in desc.lower()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
