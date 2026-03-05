"""Utilities for loading system prompts from configuration files."""

import re
from pathlib import Path
from typing import Optional


_PROMPTS_DIR = Path(__file__).parent
_TEMPLATES_DIR = _PROMPTS_DIR / "templates"


def _strip_frontmatter(content: str) -> str:
    """Strip YAML frontmatter from markdown content.

    Frontmatter is delimited by <!-- ... --> at the start of the file.

    Args:
        content: Raw file content

    Returns:
        Content with frontmatter removed
    """
    # Match <!-- ... --> at start of file
    pattern = r'^\s*<!--.*?-->\s*'
    return re.sub(pattern, '', content, flags=re.DOTALL).strip()


def get_prompt_path(prompt_name: str) -> Path:
    """Get the path to a prompt file.

    Prefers .md format, falls back to .txt for backward compatibility.

    Args:
        prompt_name: Name of the prompt (e.g., "main_system_prompt", "planner_system_prompt")

    Returns:
        Path to the prompt file in the templates directory
    """
    # Try .md first (preferred format)
    md_path = _TEMPLATES_DIR / f"{prompt_name}.md"
    if md_path.exists():
        return md_path

    # Fall back to .txt (legacy format)
    return _TEMPLATES_DIR / f"{prompt_name}.txt"


def load_prompt(prompt_name: str, fallback: Optional[str] = None) -> str:
    """Load a system prompt from file.

    Supports both .md (preferred) and .txt (legacy) formats.
    Strips YAML frontmatter from .md files.

    Args:
        prompt_name: Name of the prompt file (without extension)
        fallback: Optional fallback text if file doesn't exist

    Returns:
        The prompt text

    Raises:
        FileNotFoundError: If prompt file doesn't exist and no fallback provided
    """
    prompt_file = get_prompt_path(prompt_name)

    if not prompt_file.exists():
        if fallback is not None:
            return fallback
        raise FileNotFoundError(f"Prompt file not found: {prompt_file}")

    content = prompt_file.read_text(encoding="utf-8")

    # Strip frontmatter if .md file
    if prompt_file.suffix == ".md":
        content = _strip_frontmatter(content)

    return content.strip()


def load_tool_description(tool_name: str) -> str:
    """Load a tool description from its markdown template.

    Args:
        tool_name: The tool's function name (e.g., "write_file")

    Returns:
        The tool description text (frontmatter stripped)
    """
    kebab_name = tool_name.replace("_", "-")
    return load_prompt(f"tools/tool-{kebab_name}")


def save_prompt(prompt_name: str, content: str) -> None:
    """Save a prompt to file (useful for customization).

    Args:
        prompt_name: Name of the prompt file (without .txt extension)
        content: Prompt content to save
    """
    prompt_file = get_prompt_path(prompt_name)
    prompt_file.write_text(content, encoding="utf-8")
