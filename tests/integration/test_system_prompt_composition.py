"""Integration tests for system prompt composition."""

import pytest
from pathlib import Path
from swecli.core.agents.components.prompts.builders import (
    SystemPromptBuilder,
    ThinkingPromptBuilder,
)


def test_system_prompt_builder_loads_real_prompt():
    """Verify SystemPromptBuilder loads actual main_system_prompt.txt."""
    builder = SystemPromptBuilder(
        tool_registry=None,
        working_dir=None,
        skill_loader=None,
        subagent_manager=None,
        env_context=None,
    )

    prompt = builder.build()

    # Verify main prompt content is loaded
    assert "OpenDev" in prompt
    assert "AI software engineering assistant" in prompt or "software engineer" in prompt
    assert len(prompt) > 100  # Non-empty


def test_system_prompt_includes_environment_section():
    """Verify environment context is included."""
    from swecli.core.agents.components.prompts.environment import EnvironmentContext

    env = EnvironmentContext(
        working_dir=str(Path.cwd()),
        platform="darwin",
        os_version="Darwin 25.2.0",
        current_date="2026-02-16",
        model="gpt-4o",
        model_provider="openai",
        is_git_repo=True,
        git_branch="main",
        git_status="On branch main\nnothing to commit, working tree clean",
        project_instructions=None,
    )

    builder = SystemPromptBuilder(
        tool_registry=None,
        working_dir=Path.cwd(),
        skill_loader=None,
        subagent_manager=None,
        env_context=env,
    )

    prompt = builder.build()

    # Verify environment section is included
    assert "Working" in prompt or "directory" in prompt
    assert "main" in prompt  # git branch


def test_thinking_prompt_builder_composition():
    """Verify ThinkingPromptBuilder loads thinking_system_prompt.txt."""
    builder = ThinkingPromptBuilder(
        tool_registry=None,
        working_dir=None,
        skill_loader=None,
        subagent_manager=None,
        env_context=None,
    )

    prompt = builder.build()

    # Verify thinking-specific content is loaded
    assert len(prompt) > 50
    # Thinking mode should have some unique content
    assert "thinking" in prompt.lower() or "reason" in prompt.lower()


def test_system_prompt_with_project_instructions():
    """Verify project instructions (SWECLI.md) are included when available."""
    from swecli.core.agents.components.prompts.environment import EnvironmentContext

    env = EnvironmentContext(
        working_dir=str(Path.cwd()),
        platform="darwin",
        os_version="Darwin 25.2.0",
        current_date="2026-02-16",
        model="gpt-4o",
        model_provider="openai",
        is_git_repo=False,
        project_instructions="# Test Project\nThese are test instructions.",
    )

    builder = SystemPromptBuilder(
        tool_registry=None,
        working_dir=Path.cwd(),
        skill_loader=None,
        subagent_manager=None,
        env_context=env,
    )

    prompt = builder.build()

    # Verify project instructions are included
    assert "Test Project" in prompt
    assert "test instructions" in prompt


def test_prompt_builders_return_non_empty_strings():
    """Ensure all builders return valid non-empty prompts."""
    # System prompt
    system_builder = SystemPromptBuilder(None, None, None, None, None)
    system_prompt = system_builder.build()
    assert isinstance(system_prompt, str)
    assert len(system_prompt) > 0

    # Thinking prompt
    thinking_builder = ThinkingPromptBuilder(None, None, None, None, None)
    thinking_prompt = thinking_builder.build()
    assert isinstance(thinking_prompt, str)
    assert len(thinking_prompt) > 0



def test_environment_context_optional():
    """Verify builders work without environment context."""
    # All builders should handle None env_context gracefully
    system_builder = SystemPromptBuilder(None, None, None, None, None)
    assert system_builder.build()  # Should not raise

    thinking_builder = ThinkingPromptBuilder(None, None, None, None, None)
    assert thinking_builder.build()  # Should not raise
