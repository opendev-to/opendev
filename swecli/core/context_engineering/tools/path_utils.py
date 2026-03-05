"""Utility helpers for working with tool-related file paths."""

from __future__ import annotations


def sanitize_path(path: str) -> str:
    """Remove mention prefixes ("@"/"#") from file paths used in tools."""
    if not path:
        return path

    # Check if path starts with / (absolute path) before processing
    is_absolute = path.startswith("/")

    # Trim leading mention markers
    while path and path[0] in {"@", "#"}:
        path = path[1:]

    # Check again after removing markers in case we have @/path or #/path
    if not is_absolute and path.startswith("/"):
        is_absolute = True

    # Remove markers within path components
    parts = []
    for component in path.split("/"):
        while component and component[0] in {"@", "#"}:
            component = component[1:]
        if component:  # Only add non-empty components
            parts.append(component)

    # Reconstruct path, preserving leading / for absolute paths
    if not parts:
        return path

    result = "/".join(parts)
    if is_absolute and not result.startswith("/"):
        result = "/" + result

    return result
