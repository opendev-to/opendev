"""Integration tests for tool description rendering."""

import pytest
from pathlib import Path


def test_present_plan_tool_description_loaded():
    """Verify present_plan description loads from markdown file."""
    from swecli.core.agents.prompts.loader import load_tool_description

    description = load_tool_description("present_plan")

    # Verify content from tool-present-plan.md
    assert "How This Tool Works" in description
    assert "present_plan" in description or "plan" in description.lower()
    assert len(description) > 100

    # Verify variable substitution worked
    assert "${" not in description  # No unresolved variables


def test_tool_description_template_files_exist():
    """Verify tool description template files exist."""
    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates/tools"
    )

    # Check for present_plan tool description
    template = templates_dir / "tool-present-plan.md"
    assert template.exists(), f"Missing template: {template}"


def test_tool_description_templates_valid_markdown():
    """Verify tool description templates are valid markdown."""
    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates/tools"
    )

    for template_file in templates_dir.glob("*.md"):
        content = template_file.read_text()

        # Should have content
        assert len(content) > 0, f"Empty template: {template_file}"

        # Should have frontmatter or markdown content
        assert "<!--" in content or "#" in content or "**" in content

        # Should not have Python code
        assert "def " not in content
        assert "import " not in content


def test_present_plan_tool_name_correct():
    """Verify present_plan tool name is correctly set."""
    from swecli.core.context_engineering.tools.implementations.present_plan_tool import (
        PresentPlanTool,
    )

    tool = PresentPlanTool()
    assert tool.name == "present_plan"


def test_all_builtin_tools_have_template_files():
    """Verify every tool in _BUILTIN_TOOL_SCHEMAS has a corresponding .md template."""
    from swecli.core.agents.components.schemas.definitions import _BUILTIN_TOOL_SCHEMAS

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates/tools"
    )

    for schema in _BUILTIN_TOOL_SCHEMAS:
        tool_name = schema["function"]["name"]
        kebab_name = tool_name.replace("_", "-")
        template_path = templates_dir / f"tool-{kebab_name}.md"
        assert template_path.exists(), f"Missing template for tool '{tool_name}': {template_path}"


def test_load_tool_description_returns_content():
    """Verify load_tool_description loads and strips frontmatter."""
    from swecli.core.agents.prompts.loader import load_tool_description

    desc = load_tool_description("write_file")

    assert len(desc) > 0
    assert "<!--" not in desc  # Frontmatter stripped
    assert "Create a new file" in desc


def test_load_tool_description_missing_raises():
    """Verify load_tool_description raises FileNotFoundError for missing templates."""
    from swecli.core.agents.prompts.loader import load_tool_description

    with pytest.raises(FileNotFoundError):
        load_tool_description("nonexistent_tool_that_does_not_exist")
