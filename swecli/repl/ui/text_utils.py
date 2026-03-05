"""Text utility functions for REPL UI."""


def truncate_text(text: str, limit: int) -> str:
    """Return text truncated to a sensible length.

    Args:
        text: Text to truncate
        limit: Maximum character limit

    Returns:
        Truncated text with ellipsis if needed
    """
    if len(text) <= limit:
        return text
    return text[: limit - 1] + "…"
