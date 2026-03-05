"""Text processing utilities for the Textual UI."""

from __future__ import annotations

import re
from typing import Any


def truncate_tool_output(raw_result: Any, max_lines: int = 6, max_chars: int = 400) -> str:
    """Truncate tool output to reasonable size for display.

    Args:
        raw_result: The raw tool result to truncate
        max_lines: Maximum number of lines to show
        max_chars: Maximum characters per line

    Returns:
        Truncated output string
    """
    if not raw_result:
        return ""

    text = str(raw_result)
    lines = text.splitlines()

    if len(lines) <= max_lines:
        return text

    kept = lines[:max_lines]
    omitted = len(lines) - max_lines

    truncated_lines = []
    for line in kept:
        if len(line) > max_chars:
            truncated_lines.append(line[:max_chars] + "...")
        else:
            truncated_lines.append(line)

    truncated_lines.append(f"... ({omitted} more lines)")
    return "\n".join(truncated_lines)


def normalize_console_text(text: str) -> str:
    """Normalize console text by removing carriage returns.

    Args:
        text: Raw console text

    Returns:
        Normalized text
    """
    if not text:
        return text

    # Remove carriage returns only - preserve ANSI codes for AnsiDecoder
    normalized = text.replace("\r\n", "\n").replace("\r", "\n")

    return normalized


def clean_tool_summary(summary: str) -> str:
    """Clean tool summary text for display.

    Args:
        summary: Raw summary text

    Returns:
        Cleaned summary text
    """
    if not summary:
        return summary

    # Remove extra whitespace
    cleaned = " ".join(summary.split())

    # Remove common prefixes
    prefixes = ["Result:", "Output:", "Success:"]
    for prefix in prefixes:
        if cleaned.startswith(prefix):
            cleaned = cleaned[len(prefix) :].lstrip()

    return cleaned


def is_spinner_text(plain: str) -> bool:
    """Check if text is spinner animation text.

    Args:
        plain: Plain text to check

    Returns:
        True if text is spinner animation
    """
    if not plain:
        return False

    spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    stripped = plain.strip()

    # Check if it starts with a spinner character
    if any(stripped.startswith(char) for char in spinner_chars):
        return True

    # Check for common spinner patterns
    if re.match(r"^[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]\s+", stripped):
        return True

    return False


def is_spinner_tip(plain: str) -> bool:
    """Check if text is a spinner tip message.

    Args:
        plain: Plain text to check

    Returns:
        True if text is a spinner tip
    """
    if not plain:
        return False

    stripped = plain.strip().lower()

    # Common tip patterns
    tip_patterns = [
        "tip:",
        "hint:",
        "note:",
    ]

    return any(stripped.startswith(pattern) for pattern in tip_patterns)


def summarize_error(error: str, max_length: int = 120) -> str:
    """Summarize an error message for clean display.

    Converts raw API errors, HTTP responses, and stack traces into
    concise, user-friendly messages.

    Args:
        error: Raw error message
        max_length: Maximum length of summary

    Returns:
        Clean, summarized error message
    """
    import json

    if not error:
        return "Unknown error"

    error = str(error).strip()

    # Handle HTTP errors with JSON bodies
    http_match = re.match(r"HTTP (\d+):\s*(.+)", error, re.DOTALL)
    if http_match:
        status_code = http_match.group(1)
        body = http_match.group(2).strip()

        # Try to parse JSON body for error message
        try:
            data = json.loads(body)
            # Common error message locations in API responses
            msg = None
            error_field = data.get("error")
            if isinstance(error_field, dict):
                msg = error_field.get("message")
            elif isinstance(error_field, str):
                msg = error_field
            if not msg:
                msg = data.get("error_description") or data.get("message") or data.get("detail")
            if msg and isinstance(msg, str):
                # Truncate message if too long
                if len(msg) > max_length - 15:
                    msg = msg[: max_length - 18] + "..."
                return f"HTTP {status_code}: {msg}"
        except (json.JSONDecodeError, TypeError, AttributeError):
            pass

        # HTTP status code descriptions
        status_messages = {
            "400": "Bad request",
            "401": "Authentication failed",
            "403": "Access denied",
            "404": "Not found",
            "429": "Rate limit exceeded",
            "500": "Server error",
            "502": "Bad gateway",
            "503": "Service unavailable",
        }
        if status_code in status_messages:
            return f"HTTP {status_code}: {status_messages[status_code]}"
        return f"HTTP {status_code} error"

    # Handle common error patterns
    error_patterns = [
        # API key errors
        (r"API key.*not (found|set|valid)", "API key not configured"),
        (r"invalid.*api.?key", "Invalid API key"),
        (r"authentication.*failed", "Authentication failed"),
        # Connection errors
        (r"connection.*refused", "Connection refused"),
        (r"connection.*timed?\s*out", "Connection timed out"),
        (r"name.*resolution.*failed", "DNS resolution failed"),
        (r"no.*internet", "No internet connection"),
        # File errors
        (r"file.*not.*found", "File not found"),
        (r"permission.*denied", "Permission denied"),
        (r"no such file", "File not found"),
        # Rate limiting
        (r"rate.?limit", "Rate limit exceeded"),
        (r"too many requests", "Too many requests"),
        # Timeout
        (r"request.*timeout", "Request timed out"),
        (r"timeout.*(\d+)", lambda m: f"Timed out after {m.group(1)}s"),
    ]

    error_lower = error.lower()
    for pattern, replacement in error_patterns:
        match = re.search(pattern, error_lower)
        if match:
            if callable(replacement):
                return replacement(match)
            return replacement

    # Truncate long errors cleanly
    if len(error) > max_length:
        # Try to break at a sentence or word boundary
        truncated = error[: max_length - 3]
        # Find last space or punctuation
        last_break = max(
            truncated.rfind(" "),
            truncated.rfind("."),
            truncated.rfind(","),
            truncated.rfind(":"),
        )
        if last_break > max_length // 2:
            truncated = truncated[:last_break]
        return truncated.rstrip(".,: ") + "..."

    return error


__all__ = [
    "truncate_tool_output",
    "normalize_console_text",
    "clean_tool_summary",
    "is_spinner_text",
    "is_spinner_tip",
    "summarize_error",
]
