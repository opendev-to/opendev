"""Centralized prompt reminder strings for the LLM message chain.

All runtime reminder strings (nudges, signals, memory headers, etc.) are
stored in template files under prompts/templates/ and accessed via
get_reminder(). This keeps prompt text out of Python source files.

Short/medium prompts live in reminders.md as named sections delimited by
``--- SECTION_NAME ---`` markers. Longer prompts (e.g., Docker preambles)
live in their own .txt files and are resolved by filename fallback.
"""

from pathlib import Path
from typing import Dict

_TEMPLATES_DIR = Path(__file__).parent / "templates"

# Module-level cache: parsed sections from reminders.md
_sections: Dict[str, str] | None = None


def _parse_sections() -> Dict[str, str]:
    """Parse reminders.md into a dict of {section_name: content}.

    Section delimiters are lines matching ``--- SECTION_NAME ---``.
    Content between delimiters is stripped of leading/trailing blank lines
    but internal whitespace (including newlines) is preserved.
    """
    reminders_file = _TEMPLATES_DIR / "reminders.md"
    text = reminders_file.read_text(encoding="utf-8")

    sections: Dict[str, str] = {}
    current_name: str | None = None
    current_lines: list[str] = []

    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("--- ") and stripped.endswith(" ---"):
            # Save previous section
            if current_name is not None:
                sections[current_name] = "\n".join(current_lines).strip()
            # Start new section
            current_name = stripped[4:-4].strip()
            current_lines = []
        else:
            current_lines.append(line)

    # Save last section
    if current_name is not None:
        sections[current_name] = "\n".join(current_lines).strip()

    return sections


def get_reminder(reminder_name: str, /, **kwargs: str) -> str:
    """Get a prompt reminder by name, with optional placeholder formatting.

    Looks in reminders.md sections first, then falls back to individual
    template files (e.g., docker_preamble.txt).

    Args:
        reminder_name: The reminder name (section name or filename without .txt)
        **kwargs: Placeholder values for str.format()

    Returns:
        The formatted reminder string

    Raises:
        KeyError: If the reminder name is not found in sections or files
    """
    global _sections
    if _sections is None:
        _sections = _parse_sections()

    # Try sections first
    if reminder_name in _sections:
        template = _sections[reminder_name]
        return template.format(**kwargs) if kwargs else template

    # Fall back to individual template file
    template_file = _TEMPLATES_DIR / f"{reminder_name}.txt"
    if template_file.exists():
        template = template_file.read_text(encoding="utf-8").strip()
        return template.format(**kwargs) if kwargs else template

    raise KeyError(
        f"Unknown reminder: {reminder_name!r}. "
        f"Not found in reminders.md sections or as {template_file}"
    )


def append_nudge(
    messages: list,
    content: str,
    role: str = "user",
    **extra,
) -> None:
    """Append an internal nudge message, tagged for filtering.

    All nudge messages are tagged with ``_nudge=True`` so they can be
    excluded from the thinking phase (which clones all messages).
    """
    messages.append({"role": role, "content": content, "_nudge": True, **extra})
