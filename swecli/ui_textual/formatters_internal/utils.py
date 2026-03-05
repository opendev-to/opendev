"""Utility functions and classes for formatting."""

import json
from typing import Optional, Any


class LanguageDetector:
    """Utility class for detecting programming languages from file extensions."""

    LANGUAGE_MAP = {
        ".py": "python",
        ".js": "javascript",
        ".ts": "typescript",
        ".jsx": "jsx",
        ".tsx": "tsx",
        ".json": "json",
        ".yaml": "yaml",
        ".yml": "yaml",
        ".md": "markdown",
        ".sh": "bash",
        ".bash": "bash",
        ".zsh": "zsh",
        ".fish": "fish",
        ".html": "html",
        ".css": "css",
        ".scss": "scss",
        ".go": "go",
        ".rs": "rust",
        ".java": "java",
        ".c": "c",
        ".cpp": "cpp",
        ".h": "c",
        ".hpp": "cpp",
        ".rb": "ruby",
        ".php": "php",
        ".sql": "sql",
        ".xml": "xml",
        ".toml": "toml",
        ".ini": "ini",
        ".conf": "ini",
    }

    @classmethod
    def detect(cls, ext: str) -> Optional[str]:
        """Detect programming language from file extension.

        Args:
            ext: File extension (e.g., ".py")

        Returns:
            Language name for syntax highlighting
        """
        return cls.LANGUAGE_MAP.get(ext.lower())


class SizeFormatter:
    """Utility class for formatting file sizes in human-readable format."""

    @staticmethod
    def format_size(size: int) -> str:
        """Format file size in human-readable format.

        Args:
            size: Size in bytes

        Returns:
            Formatted size string
        """
        if size < 1024:
            return f"{size} B"
        elif size < 1024 * 1024:
            return f"{size / 1024:.1f} KB"
        elif size < 1024 * 1024 * 1024:
            return f"{size / (1024 * 1024):.1f} MB"
        else:
            return f"{size / (1024 * 1024 * 1024):.1f} GB"


class ValueSummarizer:
    """Utility class for summarizing values for display."""

    @staticmethod
    def summarize(value: Any) -> str:
        """Provide a concise representation of a value for display."""
        if isinstance(value, str):
            sanitized = value.replace("\n", "\\n")
            if len(sanitized) > 80:
                sanitized = sanitized[:77] + "…"
            return sanitized

        try:
            serialized = json.dumps(value, default=str)
        except TypeError:
            serialized = str(value)

        if len(serialized) > 80:
            serialized = serialized[:77] + "…"
        return serialized


class DiffParser:
    """Utility class for parsing unified diff format."""

    @staticmethod
    def parse_unified_diff(diff_text: str):
        """Parse unified diff text into structured entries.

        Args:
            diff_text: Unified diff text

        Returns:
            List of tuples: (entry_type, line_number, content)
        """
        import re
        from typing import List, Tuple, Optional

        entries: List[Tuple[str, Optional[int], str]] = []
        old_line: Optional[int] = None
        new_line: Optional[int] = None

        hunk_pattern = re.compile(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@")

        for raw_line in diff_text.splitlines():
            if raw_line.startswith("---") or raw_line.startswith("+++"):
                continue

            if raw_line.startswith("@@"):
                match = hunk_pattern.match(raw_line)
                if match:
                    old_line = int(match.group(1))
                    new_line = int(match.group(2))
                entries.append(("hunk", None, raw_line))
                continue

            if raw_line.startswith("+"):
                content = raw_line[1:]
                entries.append(("add", new_line, content))
                if new_line is not None:
                    new_line += 1
                continue

            if raw_line.startswith("-"):
                content = raw_line[1:]
                entries.append(("del", old_line, content))
                if old_line is not None:
                    old_line += 1
                continue

            # Context line
            content = raw_line[1:] if raw_line.startswith(" ") else raw_line
            entries.append(("ctx", old_line, content))
            if old_line is not None:
                old_line += 1
            if new_line is not None:
                new_line += 1

        return entries

    @staticmethod
    def group_by_hunk(entries):
        """Group diff entries by hunk.

        Returns:
            List of (start_line, entries) tuples where entries excludes hunk headers
        """
        import re

        hunks = []
        current_entries = []
        current_start = 1
        hunk_pattern = re.compile(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@")

        for entry_type, line_no, content in entries:
            if entry_type == "hunk":
                if current_entries:
                    hunks.append((current_start, current_entries))
                    current_entries = []
                match = hunk_pattern.match(content)
                current_start = int(match.group(1)) if match else 1
            else:
                current_entries.append((entry_type, line_no, content))

        if current_entries:
            hunks.append((current_start, current_entries))

        return hunks
