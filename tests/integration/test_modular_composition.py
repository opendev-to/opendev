"""Integration tests for modular prompt composition."""

import pytest
from pathlib import Path


def test_modular_composition_loads_all_sections():
    """Verify PromptComposer loads all registered sections."""
    from swecli.core.agents.prompts.composition import create_default_composer

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates"
    )

    composer = create_default_composer(templates_dir)

    # Compose with all conditionals enabled
    context = {
        "in_git_repo": True,
        "has_subagents": True,
        "todo_tracking_enabled": True,
    }

    prompt = composer.compose(context)

    # Verify key sections are included (from main/ modules)
    assert "Security Policy" in prompt  # Security
    assert "Tone and Style" in prompt  # Tone
    assert "Git Workflow" in prompt  # Conditional: git
    assert "Task Tracking" in prompt  # Conditional: todos
    assert "Subagent Guide" in prompt  # Conditional: subagents

    # Verify prompt is substantial
    assert len(prompt) > 1000


def test_modular_composition_conditional_git():
    """Verify git workflow section is conditionally loaded."""
    from swecli.core.agents.prompts.composition import create_default_composer

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates"
    )

    composer = create_default_composer(templates_dir)

    # Without git repo
    context_no_git = {
        "in_git_repo": False,
        "has_subagents": True,
        "todo_tracking_enabled": True,
    }

    prompt_no_git = composer.compose(context_no_git)
    assert "Git Workflow" not in prompt_no_git

    # With git repo
    context_with_git = {
        "in_git_repo": True,
        "has_subagents": True,
        "todo_tracking_enabled": True,
    }

    prompt_with_git = composer.compose(context_with_git)
    assert "Git Workflow" in prompt_with_git


def test_modular_composition_priority_order():
    """Verify sections are loaded in priority order."""
    from swecli.core.agents.prompts.composition import create_default_composer

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates"
    )

    composer = create_default_composer(templates_dir)

    context = {
        "in_git_repo": True,
        "has_subagents": True,
        "todo_tracking_enabled": True,
    }

    prompt = composer.compose(context)

    # Core sections should appear before conditional sections
    core_pos = prompt.find("OpenDev")
    security_pos = prompt.find("Security Policy")
    git_pos = prompt.find("Git Workflow")

    assert core_pos < security_pos  # Priority 10 < 15
    assert security_pos < git_pos  # Priority 15 < 70


def test_system_prompt_builder_uses_modular():
    """Verify SystemPromptBuilder uses modular composition."""
    from swecli.core.agents.components.prompts.builders import SystemPromptBuilder
    from swecli.core.agents.components.prompts.environment import EnvironmentContext
    from pathlib import Path

    env = EnvironmentContext(
        working_dir=str(Path.cwd()),
        platform="darwin",
        os_version="Darwin 25.2.0",
        current_date="2026-02-16",
        model="gpt-4o",
        model_provider="openai",
        is_git_repo=True,
        git_branch="main",
    )

    builder = SystemPromptBuilder(
        tool_registry=None,
        working_dir=Path.cwd(),
        skill_loader=None,
        subagent_manager=None,
        env_context=env,
    )

    prompt = builder.build()

    # Should include modular sections
    assert "OpenDev" in prompt
    assert "Security Policy" in prompt

    # Should include git workflow (since in_git_repo=True)
    assert "Git Workflow" in prompt or "git" in prompt.lower()


def test_modular_files_have_frontmatter():
    """Verify all modular files have proper frontmatter."""
    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates/system/main"
    )

    modular_files = list(templates_dir.glob("*.md"))
    assert len(modular_files) == 21, (
        f"Expected 21 modular files in main/, found {len(modular_files)}"
    )

    for file_path in modular_files:
        content = file_path.read_text()

        # Should have frontmatter
        assert content.startswith("<!--"), f"Missing frontmatter: {file_path.name}"
        assert "name:" in content[:200], f"Missing name in frontmatter: {file_path.name}"
        assert "version:" in content[:200], f"Missing version in frontmatter: {file_path.name}"


def test_thinking_modular_files_have_frontmatter():
    """Verify all thinking modular files have proper frontmatter."""
    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates/system/thinking"
    )

    modular_files = list(templates_dir.glob("*.md"))
    assert len(modular_files) == 4, (
        f"Expected 4 modular files in thinking/, found {len(modular_files)}"
    )

    for file_path in modular_files:
        content = file_path.read_text()

        # Should have frontmatter
        assert content.startswith("<!--"), f"Missing frontmatter: {file_path.name}"
        assert "name:" in content[:200], f"Missing name in frontmatter: {file_path.name}"
        assert "version:" in content[:200], f"Missing version in frontmatter: {file_path.name}"


def test_thinking_composition_loads_sections():
    """Verify thinking composer loads all thinking-specific sections."""
    from swecli.core.agents.prompts.composition import create_thinking_composer

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates"
    )

    composer = create_thinking_composer(templates_dir)
    context = {}

    prompt = composer.compose(context)

    # Verify thinking-specific sections are included
    assert "Available Tools" in prompt
    assert "Subagent Selection Guide" in prompt
    assert "Output Rules" in prompt
    assert "Code References" in prompt

    # Verify main-only sections are NOT included
    assert "Security Policy" not in prompt
    assert "Tone and Style" not in prompt
    assert "Git Workflow" not in prompt


def test_create_composer_dispatches_correctly():
    """Verify create_composer dispatches to the right composer."""
    from swecli.core.agents.prompts.composition import create_composer

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates"
    )

    # Main mode should include main sections
    main_composer = create_composer(templates_dir, "system/main")
    main_prompt = main_composer.compose({"in_git_repo": True, "has_subagents": True})
    assert "Security Policy" in main_prompt

    # Thinking mode should include thinking sections
    thinking_composer = create_composer(templates_dir, "system/thinking")
    thinking_prompt = thinking_composer.compose({})
    assert "Output Rules" in thinking_prompt
    assert "Security Policy" not in thinking_prompt


def test_modular_composition_no_unresolved_variables():
    """Verify composed prompt has no unresolved template variables."""
    from swecli.core.agents.prompts.composition import create_default_composer

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates"
    )

    composer = create_default_composer(templates_dir)

    context = {
        "in_git_repo": True,
        "has_subagents": True,
        "todo_tracking_enabled": True,
    }

    prompt = composer.compose(context)

    # Should have no unresolved variables (frontmatter stripped)
    assert "${" not in prompt
    assert "<!--" not in prompt


def test_modular_composition_empty_context():
    """Verify composer works with empty context."""
    from swecli.core.agents.prompts.composition import create_default_composer

    templates_dir = (
        Path(__file__).parent.parent.parent
        / "swecli/core/agents/prompts/templates"
    )

    composer = create_default_composer(templates_dir)

    # Empty context - only unconditional sections should load
    context = {
        "in_git_repo": False,
        "has_subagents": False,
        "todo_tracking_enabled": False,
    }

    prompt = composer.compose(context)

    # Core sections should be present (from main/ modules)
    assert "Security Policy" in prompt

    # Conditional sections should NOT be present
    assert "Git Workflow" not in prompt
    assert "Task Tracking" not in prompt
    assert "Subagent Guide" not in prompt
