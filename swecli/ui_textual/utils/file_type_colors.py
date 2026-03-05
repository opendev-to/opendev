"""Centralized color mapping for file types in autocomplete."""

from pathlib import Path
from typing import Tuple


class FileTypeColors:
    """Maps file types to Rich color styles for autocomplete display."""

    # Color mappings for different file types
    COLOR_MAP = {
        # Programming Languages
        ".py": "bright_blue",
        ".js": "bright_yellow",
        ".jsx": "bright_yellow",
        ".ts": "dodger_blue2",
        ".tsx": "dodger_blue2",
        ".rs": "orange_red1",
        ".go": "cyan",
        ".java": "red3",
        ".c": "blue",
        ".h": "blue",
        ".cpp": "blue",
        ".cc": "blue",
        ".hpp": "blue",
        ".cs": "green",
        ".rb": "red",
        ".php": "purple",
        ".swift": "orange3",
        ".kt": "purple",
        ".kts": "purple",

        # Web Files
        ".html": "orange3",
        ".htm": "orange3",
        ".css": "dodger_blue2",
        ".scss": "hot_pink",
        ".sass": "hot_pink",
        ".less": "blue",
        ".json": "bright_yellow",
        ".xml": "green",
        ".yaml": "purple",
        ".yml": "purple",
        ".toml": "purple",

        # Documentation
        ".md": "bright_cyan",
        ".markdown": "bright_cyan",
        ".txt": "grey70",
        ".pdf": "red",
        ".doc": "blue",
        ".docx": "blue",
        ".rst": "bright_cyan",

        # Images
        ".png": "bright_magenta",
        ".jpg": "bright_magenta",
        ".jpeg": "bright_magenta",
        ".gif": "bright_magenta",
        ".svg": "magenta",
        ".ico": "bright_magenta",
        ".webp": "bright_magenta",
        ".bmp": "bright_magenta",

        # Data Files
        ".csv": "green",
        ".sql": "orange3",
        ".db": "cyan",
        ".sqlite": "cyan",
        ".sqlite3": "cyan",

        # Archives
        ".zip": "red",
        ".tar": "red",
        ".gz": "red",
        ".bz2": "red",
        ".7z": "red",
        ".rar": "red",
        ".tgz": "red",

        # Shell Scripts
        ".sh": "green",
        ".bash": "green",
        ".zsh": "green",
        ".fish": "green",

        # Configuration
        ".env": "yellow",
        ".ini": "grey70",
        ".conf": "grey70",
        ".config": "grey70",
        ".lock": "grey50",

        # Build Files
        ".gradle": "green",
        ".maven": "orange3",

        # Special files
        "makefile": "red",
        "dockerfile": "dodger_blue2",
        "vagrantfile": "dodger_blue2",
        "readme": "bright_cyan",
        "license": "yellow",
        "changelog": "bright_cyan",
        ".gitignore": "grey70",
        ".dockerignore": "grey70",

        # Version Control
        ".git": "orange3",

        # Package files
        "package.json": "green",
        "cargo.toml": "orange_red1",
        "pyproject.toml": "bright_blue",
        "requirements.txt": "bright_blue",
        "gemfile": "red",
        "podfile": "red",
    }

    # Special directory color
    DIRECTORY_COLOR = "bright_blue"

    # Default fallback color
    DEFAULT_COLOR = "white"

    @classmethod
    def get_color_for_path(cls, path_str: str, is_dir: bool = False) -> str:
        """Get Rich color style for a file path.

        Args:
            path_str: Path string
            is_dir: Whether this is a directory

        Returns:
            Rich color style string
        """
        if is_dir:
            return cls.DIRECTORY_COLOR

        # Try to extract path from the string
        # Handle cases like "[py] script.py" or just "script.py"
        clean_path = path_str

        # Remove icon brackets if present
        if "]" in clean_path:
            parts = clean_path.split("]", 1)
            if len(parts) > 1:
                clean_path = parts[1].strip()

        # Convert to Path for easier manipulation
        try:
            path = Path(clean_path)

            # Check special filenames (case-insensitive)
            name_lower = path.name.lower()
            if name_lower in cls.COLOR_MAP:
                return cls.COLOR_MAP[name_lower]

            # Check extension
            suffix = path.suffix.lower()
            if suffix in cls.COLOR_MAP:
                return cls.COLOR_MAP[suffix]

            # Check if it's a common name without extension
            stem_lower = path.stem.lower()
            if stem_lower in cls.COLOR_MAP:
                return cls.COLOR_MAP[stem_lower]

        except (ValueError, OSError):
            pass

        return cls.DEFAULT_COLOR

    @classmethod
    def get_color_from_extension(cls, extension: str) -> str:
        """Get color directly from file extension.

        Args:
            extension: File extension (with or without dot)

        Returns:
            Rich color style string
        """
        ext = extension if extension.startswith(".") else f".{extension}"
        return cls.COLOR_MAP.get(ext.lower(), cls.DEFAULT_COLOR)

    @classmethod
    def get_color_from_icon_label(cls, label: str) -> str:
        """Extract file type from label and return color.

        Args:
            label: Label like '• script.py' or '[py] script.py' or just 'script.py'

        Returns:
            Rich color style string
        """
        # Remove common prefixes (colored dot, old bracket style)
        clean_label = label.strip()

        # Remove colored dot prefix if present
        if clean_label.startswith("•"):
            clean_label = clean_label[1:].strip()

        # Handle old bracket style for backward compatibility
        if clean_label.startswith("[") and "]" in clean_label:
            icon_end = clean_label.index("]")
            icon_text = clean_label[1:icon_end].strip()

            # Try to get color from icon text
            color = cls.get_color_from_extension(icon_text)
            if color != cls.DEFAULT_COLOR:
                return color

            # Also check the actual filename part
            filename_part = clean_label[icon_end + 1:].strip()
            return cls.get_color_for_path(filename_part)

        # No prefix, just check the label as filename
        return cls.get_color_for_path(clean_label)
