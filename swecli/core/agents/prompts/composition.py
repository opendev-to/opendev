"""Prompt composition engine with conditional loading.

This module provides a flexible system for composing system prompts from
modular sections based on runtime context and conversation lifecycle.
"""

from pathlib import Path
from typing import List, Optional, Callable, Dict, Any
from dataclasses import dataclass
import re


@dataclass
class PromptSection:
    """A section to conditionally include in the system prompt."""

    name: str
    file_path: str
    condition: Optional[Callable[[Dict[str, Any]], bool]] = None
    priority: int = 50  # Lower = earlier in prompt
    cacheable: bool = True  # True = stable (cacheable), False = dynamic (changes per session)


class PromptComposer:
    """Composes system prompts from modular sections.

    This follows Claude Code's approach of building prompts from many small
    markdown files with conditional loading based on runtime context.

    Example:
        >>> composer = PromptComposer(templates_dir)
        >>> composer.register_section(
        ...     "security_policy",
        ...     "system/main/security-policy.md",
        ...     priority=10
        ... )
        >>> composer.register_section(
        ...     "git_workflow",
        ...     "system/main/git-workflow.md",
        ...     condition=lambda ctx: ctx.get("in_git_repo", False),
        ...     priority=70
        ... )
        >>> context = {"in_git_repo": True}
        >>> prompt = composer.compose(context)
    """

    def __init__(self, templates_dir: Path):
        """Initialize composer.

        Args:
            templates_dir: Directory containing template files
        """
        self.templates_dir = templates_dir
        self._sections: List[PromptSection] = []

    def register_section(
        self,
        name: str,
        file_path: str,
        condition: Optional[Callable[[Dict[str, Any]], bool]] = None,
        priority: int = 50,
        cacheable: bool = True,
    ):
        """Register a prompt section for conditional inclusion.

        Args:
            name: Section identifier
            file_path: Path to template file (relative to templates_dir)
            condition: Optional predicate to determine if section should be included
            priority: Loading priority (lower = earlier, default 50)
            cacheable: Whether this section is stable across turns (True) or dynamic (False).
                Stable sections can benefit from Anthropic prompt caching.
        """
        self._sections.append(PromptSection(name, file_path, condition, priority, cacheable))

    def compose(self, context: Dict[str, Any]) -> str:
        """Compose final prompt from registered sections.

        Args:
            context: Runtime context for evaluating conditions

        Returns:
            Composed prompt string
        """
        # Filter sections by condition
        included = [
            s for s in self._sections if s.condition is None or s.condition(context)
        ]

        # Sort by priority
        included.sort(key=lambda s: s.priority)

        # Load and concatenate
        parts = []
        for section in included:
            file_path = self.templates_dir / section.file_path
            if file_path.exists():
                content = self._load_section(file_path)
                if content:  # Skip empty sections
                    parts.append(content)

        return "\n\n".join(parts)

    def compose_two_part(self, context: Dict[str, Any]) -> tuple[str, str]:
        """Compose prompt split into stable (cacheable) and dynamic parts.

        For Anthropic prompt caching: the stable part gets cache_control,
        the dynamic part changes per session/turn.

        Args:
            context: Runtime context for evaluating conditions

        Returns:
            Tuple of (stable_prompt, dynamic_prompt). Either may be empty.
        """
        included = [
            s for s in self._sections if s.condition is None or s.condition(context)
        ]
        included.sort(key=lambda s: s.priority)

        stable_parts = []
        dynamic_parts = []
        for section in included:
            file_path = self.templates_dir / section.file_path
            if file_path.exists():
                content = self._load_section(file_path)
                if content:
                    if section.cacheable:
                        stable_parts.append(content)
                    else:
                        dynamic_parts.append(content)

        return "\n\n".join(stable_parts), "\n\n".join(dynamic_parts)

    def _load_section(self, file_path: Path) -> str:
        """Load a section file and strip frontmatter.

        Args:
            file_path: Path to section file

        Returns:
            Section content with frontmatter removed
        """
        content = file_path.read_text(encoding="utf-8")

        # Strip YAML frontmatter (<!-- ... -->)
        content = re.sub(r"^\s*<!--.*?-->\s*", "", content, flags=re.DOTALL)

        return content.strip()


def create_default_composer(templates_dir: Path) -> PromptComposer:
    """Create composer with default sections registered.

    This follows the priority order:
    - 10-30: Core identity and policies (always loaded)
    - 40-50: Tool guidance and interaction patterns
    - 60-80: Conditional sections (git, MCP, etc.)
    - 90+: Context-specific additions

    Args:
        templates_dir: Directory containing template files

    Returns:
        Configured PromptComposer
    """
    composer = PromptComposer(templates_dir)

    # Core sections (always included) - Priority 10-30
    composer.register_section(
        "mode_awareness", "system/main/main-mode-awareness.md", priority=12
    )

    composer.register_section(
        "security_policy", "system/main/main-security-policy.md", priority=15
    )

    composer.register_section(
        "tone_and_style", "system/main/main-tone-and-style.md", priority=20
    )

    composer.register_section(
        "no_time_estimates",
        "system/main/main-no-time-estimates.md",
        priority=25,
    )

    # Interaction patterns - Priority 40-50
    composer.register_section(
        "interaction_pattern",
        "system/main/main-interaction-pattern.md",
        priority=40,
    )

    composer.register_section(
        "available_tools", "system/main/main-available-tools.md", priority=45
    )

    composer.register_section(
        "tool_selection", "system/main/main-tool-selection.md", priority=50
    )

    # Code quality and workflows - Priority 55-65
    composer.register_section(
        "code_quality", "system/main/main-code-quality.md", priority=55
    )

    composer.register_section(
        "action_safety", "system/main/main-action-safety.md", priority=56
    )

    composer.register_section(
        "read_before_edit",
        "system/main/main-read-before-edit.md",
        priority=58,
    )

    composer.register_section(
        "error_recovery", "system/main/main-error-recovery.md", priority=60
    )

    # Conditional sections - Priority 70-80
    composer.register_section(
        "git_workflow",
        "system/main/main-git-workflow.md",
        condition=lambda ctx: ctx.get("in_git_repo", False),
        priority=70,
    )

    composer.register_section(
        "task_tracking",
        "system/main/main-task-tracking.md",
        condition=lambda ctx: ctx.get("todo_tracking_enabled", False),
        priority=75,
    )

    composer.register_section(
        "subagent_guide",
        "system/main/main-subagent-guide.md",
        condition=lambda ctx: ctx.get("has_subagents", False),
        priority=65,
    )

    # Provider-specific sections - Priority 80
    composer.register_section(
        "provider_openai",
        "system/main/main-provider-openai.md",
        condition=lambda ctx: ctx.get("model_provider") == "openai",
        priority=80,
    )
    composer.register_section(
        "provider_anthropic",
        "system/main/main-provider-anthropic.md",
        condition=lambda ctx: ctx.get("model_provider") == "anthropic",
        priority=80,
    )
    composer.register_section(
        "provider_fireworks",
        "system/main/main-provider-fireworks.md",
        condition=lambda ctx: ctx.get("model_provider") in ("fireworks", "fireworks-ai"),
        priority=80,
    )

    # Context awareness - Priority 85-95
    # NOTE: Scratchpad and reminders are dynamic (change per session/turn),
    # so they're marked cacheable=False for Anthropic prompt caching.
    composer.register_section(
        "scratchpad",
        "system/main/main-scratchpad.md",
        condition=lambda ctx: ctx.get("session_id") is not None,
        priority=87,
        cacheable=False,
    )

    composer.register_section(
        "output_awareness",
        "system/main/main-output-awareness.md",
        priority=85,
    )

    composer.register_section(
        "code_references", "system/main/main-code-references.md", priority=90
    )

    composer.register_section(
        "system_reminders_note",
        "system/main/main-reminders-note.md",
        priority=95,
        cacheable=False,
    )

    return composer


def create_thinking_composer(templates_dir: Path) -> PromptComposer:
    """Create composer with thinking-mode sections registered.

    Thinking mode is a reasoning pre-phase with no tool execution,
    so it uses purpose-built sections rather than the main 16.

    Args:
        templates_dir: Directory containing template files

    Returns:
        Configured PromptComposer for thinking mode
    """
    composer = PromptComposer(templates_dir)

    composer.register_section(
        "available_tools", "system/thinking/thinking-available-tools.md", priority=45
    )

    composer.register_section(
        "subagent_guide", "system/thinking/thinking-subagent-guide.md", priority=50
    )

    composer.register_section(
        "code_references", "system/thinking/thinking-code-references.md", priority=85
    )

    composer.register_section(
        "output_rules", "system/thinking/thinking-output-rules.md", priority=90
    )

    return composer


def create_composer(templates_dir: Path, mode: str = "system/main") -> PromptComposer:
    """Create the appropriate composer for a given mode.

    Args:
        templates_dir: Directory containing template files
        mode: The core template path (e.g. "system/main", "system/thinking")

    Returns:
        Configured PromptComposer for the specified mode
    """
    if mode == "system/thinking":
        return create_thinking_composer(templates_dir)
    return create_default_composer(templates_dir)
