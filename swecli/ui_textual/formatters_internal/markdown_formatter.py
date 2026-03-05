"""Utilities for converting Markdown into simple plain-text formatting."""

from __future__ import annotations

import re
from typing import Iterable


def _strip_emphasis(text: str) -> str:
    """Remove common Markdown emphasis markers."""
    patterns: Iterable[tuple[str, str]] = (
        (r"\*\*(.*?)\*\*", r"\1"),
        (r"__(.*?)__", r"\1"),
        (r"\*(.*?)\*", r"\1"),
        (r"_(.*?)_", r"\1"),
        (r"`([^`]*)`", r"\1"),
    )
    cleaned = text
    for pattern, replacement in patterns:
        cleaned = re.sub(pattern, replacement, cleaned)
    return cleaned


def markdown_to_plain_text(content: str) -> str:
    """Convert Markdown content to a lightly formatted plain-text string."""
    lines = content.splitlines()
    output: list[str] = []
    in_code_block = False
    code_buffer: list[str] = []
    index = 0
    total = len(lines)

    def flush_code_block() -> None:
        if not code_buffer:
            return
        if output and output[-1] != "":
            output.append("")
        for line in code_buffer:
            output.append(f"    {line}")
        output.append("")
        code_buffer.clear()

    while index < total:
        raw_line = lines[index]
        stripped = raw_line.strip()

        if stripped.startswith("```"):
            if in_code_block:
                flush_code_block()
                in_code_block = False
            else:
                in_code_block = True
                code_buffer.clear()
            index += 1
            continue

        if in_code_block:
            code_buffer.append(raw_line.rstrip())
            index += 1
            continue

        if not stripped:
            if output and output[-1] != "":
                output.append("")
            index += 1
            continue

        if re.fullmatch(r"[\-\*_]{3,}", stripped):
            output.append("────────────────────────────────────────")
            index += 1
            continue

        if stripped.startswith("#"):
            level = len(stripped) - len(stripped.lstrip('#'))
            title = _strip_emphasis(stripped[level:].strip())
            if output and output[-1] != "":
                output.append("")
            if level <= 2:
                title = title.upper()
            output.append(title)
            output.append("")
            index += 1
            continue

        if stripped.startswith(">"):
            quote_lines: list[str] = []
            while index < total and lines[index].strip().startswith(">"):
                quote_lines.append(lines[index].lstrip("> ").rstrip())
                index += 1
            quote_text = _strip_emphasis(" ".join(quote_lines).strip())
            if quote_text:
                output.append(f" ❝ {quote_text}")
            continue

        bullet_match = re.match(r"^(\s*)[-*+]\s+(.*)", raw_line)
        if bullet_match:
            indent = bullet_match.group(1) or ""
            indent_level = max(0, len(indent) // 2)
            bullet_text = _strip_emphasis(bullet_match.group(2).strip())
            symbol = "•" if indent_level == 0 else "–"
            prefix = "" if indent_level == 0 else "  " * indent_level
            output.append(f"{prefix}{symbol} {bullet_text}")
            index += 1
            continue

        ordered_match = re.match(r"^(\s*)(\d+)[.)]\s+(.*)", raw_line)
        if ordered_match:
            indent = ordered_match.group(1) or ""
            indent_level = max(0, len(indent) // 2)
            number = ordered_match.group(2)
            bullet_text = _strip_emphasis(ordered_match.group(3).strip())
            if indent_level == 0:
                output.append(f"{number}. {bullet_text}")
            else:
                output.append(f"{'  ' * indent_level}– {bullet_text}")
            index += 1
            continue

        cleaned_line = _strip_emphasis(raw_line.rstrip())
        output.append(cleaned_line)
        index += 1

    if in_code_block:
        flush_code_block()

    # Collapse excessive blank lines but preserve single spacing
    cleaned_output: list[str] = []
    previous_blank = False
    for line in output:
        if not line.strip():
            if not previous_blank and cleaned_output:
                cleaned_output.append("")
            previous_blank = True
        else:
            cleaned_output.append(line.rstrip())
            previous_blank = False

    return "\n".join(cleaned_output).strip()
