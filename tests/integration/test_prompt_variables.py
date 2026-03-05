"""Integration tests for prompt variable substitution."""

import pytest
from pathlib import Path


def test_variable_substitution_in_real_template():
    """Test variable substitution with actual template file."""
    from swecli.core.agents.prompts.renderer import PromptRenderer

    renderer = PromptRenderer()

    # Use actual reminder template
    template_path = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates/reminders/reminder-plan-mode-active.md"
    )

    if not template_path.exists():
        pytest.skip(f"Template not found: {template_path}")

    result = renderer.render(template_path)

    # Verify variables were substituted
    assert "exit_plan_mode" in result  # ${EXIT_PLAN_MODE_TOOL.name}
    assert "ask_user" in result  # ${ASK_USER_QUESTION_TOOL_NAME}
    assert "3" in result  # ${EXPLORE_AGENT_COUNT}

    # Verify frontmatter was stripped
    assert "<!--" not in result
    assert "name:" not in result or "name:" not in result[:200]  # Not at the start


def test_all_tool_variables_resolve():
    """Verify all tool variables in registry resolve correctly."""
    from swecli.core.agents.prompts.variables import PromptVariables

    variables = PromptVariables()
    var_dict = variables.to_dict()

    # Check all tool variables
    assert var_dict["EDIT_TOOL"].name == "edit_file"
    assert var_dict["WRITE_TOOL"].name == "write_file"
    assert var_dict["READ_TOOL"].name == "read_file"
    assert var_dict["BASH_TOOL"].name == "run_command"
    assert var_dict["GLOB_TOOL"].name == "list_files"
    assert var_dict["GREP_TOOL"].name == "search"
    assert var_dict["PRESENT_PLAN_TOOL"].name == "present_plan"
    assert var_dict["ASK_USER_QUESTION_TOOL_NAME"] == "ask_user"


def test_agent_count_variables():
    """Verify agent count variables are set."""
    from swecli.core.agents.prompts.variables import PromptVariables

    variables = PromptVariables()
    var_dict = variables.to_dict()

    assert var_dict["EXPLORE_AGENT_COUNT"] == 3
    assert var_dict["PLAN_AGENT_COUNT"] == 1


def test_system_reminder_variable():
    """Verify system reminder variable includes plan file context."""
    from swecli.core.agents.prompts.variables import PromptVariables
    import tempfile
    import os

    variables = PromptVariables()

    # Test with existing file
    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        plan_path = f.name
        f.write("# Test Plan\n")

    try:
        reminder_var = variables.get_system_reminder(plan_path)
        assert reminder_var.planFilePath == plan_path
        assert reminder_var.planExists is True
    finally:
        os.unlink(plan_path)

    # Test with non-existing file
    reminder_var = variables.get_system_reminder("/nonexistent/plan.md")
    assert reminder_var.planFilePath == "/nonexistent/plan.md"
    assert reminder_var.planExists is False


def test_runtime_variables_merge():
    """Verify runtime variables can be merged into variable dict."""
    from swecli.core.agents.prompts.variables import PromptVariables

    variables = PromptVariables()

    # Merge additional runtime variables
    var_dict = variables.to_dict(
        CUSTOM_VAR="custom_value",
        ANOTHER_VAR=42,
    )

    # Base variables should be present
    assert "EDIT_TOOL" in var_dict
    assert "BASH_TOOL" in var_dict

    # Runtime variables should be merged
    assert var_dict["CUSTOM_VAR"] == "custom_value"
    assert var_dict["ANOTHER_VAR"] == 42


def test_prompt_renderer_integration():
    """Verify PromptRenderer uses PromptVariables correctly."""
    from swecli.core.agents.prompts.renderer import PromptRenderer

    renderer = PromptRenderer()

    # Check that renderer has variables
    assert hasattr(renderer, "variables")

    # Verify variables are PromptVariables instance
    from swecli.core.agents.prompts.variables import PromptVariables

    assert isinstance(renderer.variables, PromptVariables)


def test_template_variable_syntax():
    """Verify template variable syntax is correctly processed."""
    from swecli.core.agents.prompts.renderer import PromptRenderer
    import tempfile

    renderer = PromptRenderer()

    # Create temporary template with variables
    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        template_path = Path(f.name)
        f.write("""<!--
name: Test Template
-->
Tool: ${EDIT_TOOL.name}
Command: ${BASH_TOOL.name}
Count: ${EXPLORE_AGENT_COUNT}
""")

    try:
        result = renderer.render(template_path)

        # Verify substitutions
        assert "edit_file" in result
        assert "run_command" in result
        assert "3" in result

        # No unresolved variables
        assert "${" not in result
    finally:
        template_path.unlink()


def test_nested_variable_access():
    """Verify nested variable access (e.g., TOOL.name) works."""
    from swecli.core.agents.prompts.variables import PromptVariables

    variables = PromptVariables()

    # Tool variables should have .name attribute
    assert hasattr(variables.EDIT_TOOL, "name")
    assert hasattr(variables.BASH_TOOL, "name")
    assert hasattr(variables.PRESENT_PLAN_TOOL, "name")

    # Verify names are correct
    assert variables.EDIT_TOOL.name == "edit_file"
    assert variables.BASH_TOOL.name == "run_command"
    assert variables.PRESENT_PLAN_TOOL.name == "present_plan"


def test_no_unresolved_variables_in_tool_descriptions():
    """Verify tool descriptions have no unresolved template variables."""
    from swecli.core.agents.prompts.loader import load_tool_description

    present_desc = load_tool_description("present_plan")

    # Descriptions should have all variables resolved
    assert "${" not in present_desc

    # Should not have template artifacts
    assert "<!--" not in present_desc
