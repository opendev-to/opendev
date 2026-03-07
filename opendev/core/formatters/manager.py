"""Auto-formatting support for edited files."""

from __future__ import annotations

import logging
import shutil
import subprocess
from pathlib import Path
from typing import Optional

logger = logging.getLogger(__name__)


class FormatterInfo:
    """Information about an available formatter."""

    def __init__(self, name: str, command: str, extensions: list[str], available: bool):
        self.name = name
        self.command = command
        self.extensions = extensions
        self.available = available

    def __repr__(self) -> str:
        status = "available" if self.available else "not found"
        return f"Formatter({self.name}, {status})"


# Formatter definitions: name -> (command, args_template, extensions)
# {file} is replaced with the actual file path
_FORMATTERS = {
    "black": {
        "command": "black",
        "args": ["--quiet", "--line-length", "100", "{file}"],
        "extensions": [".py"],
    },
    "prettier": {
        "command": "prettier",
        "args": ["--write", "{file}"],
        "extensions": [
            ".js",
            ".jsx",
            ".ts",
            ".tsx",
            ".css",
            ".scss",
            ".html",
            ".json",
            ".md",
            ".yaml",
            ".yml",
        ],
    },
    "gofmt": {
        "command": "gofmt",
        "args": ["-w", "{file}"],
        "extensions": [".go"],
    },
    "rustfmt": {
        "command": "rustfmt",
        "args": ["{file}"],
        "extensions": [".rs"],
    },
    "clang-format": {
        "command": "clang-format",
        "args": ["-i", "{file}"],
        "extensions": [".c", ".cpp", ".h", ".hpp", ".cc", ".cxx"],
    },
    "shfmt": {
        "command": "shfmt",
        "args": ["-w", "{file}"],
        "extensions": [".sh", ".bash"],
    },
    "isort": {
        "command": "isort",
        "args": ["--quiet", "{file}"],
        "extensions": [".py"],
    },
}


class FormatterManager:
    """Detects and runs formatters on edited files."""

    def __init__(self, working_dir: Path, enabled: bool = True):
        self._working_dir = working_dir
        self._enabled = enabled
        self._available: dict[str, FormatterInfo] | None = None
        self._disabled_formatters: set[str] = set()

    def detect_formatters(self) -> list[FormatterInfo]:
        """Detect which formatters are available on the system."""
        if self._available is not None:
            return list(self._available.values())

        self._available = {}
        for name, config in _FORMATTERS.items():
            cmd = config["command"]
            available = shutil.which(cmd) is not None
            info = FormatterInfo(
                name=name,
                command=cmd,
                extensions=config["extensions"],
                available=available,
            )
            self._available[name] = info

        found = [f for f in self._available.values() if f.available]
        if found:
            logger.debug(
                "Detected %d formatters: %s",
                len(found),
                ", ".join(f.name for f in found),
            )
        return list(self._available.values())

    def get_formatter_for_file(self, file_path: str) -> FormatterInfo | None:
        """Get the best formatter for a given file."""
        self.detect_formatters()
        ext = Path(file_path).suffix.lower()

        for name, info in (self._available or {}).items():
            if name in self._disabled_formatters:
                continue
            if info.available and ext in info.extensions:
                return info
        return None

    def format_file(self, file_path: str) -> bool:
        """Run the appropriate formatter on a file.

        Args:
            file_path: Path to the file to format.

        Returns:
            True if formatting was applied, False otherwise.
        """
        if not self._enabled:
            return False

        formatter = self.get_formatter_for_file(file_path)
        if not formatter:
            return False

        config = _FORMATTERS.get(formatter.name)
        if not config:
            return False

        args = [arg.replace("{file}", str(file_path)) for arg in config["args"]]
        cmd = [config["command"]] + args

        try:
            result = subprocess.run(
                cmd,
                cwd=str(self._working_dir),
                capture_output=True,
                text=True,
                timeout=10,
            )
            if result.returncode == 0:
                logger.debug("Formatted %s with %s", file_path, formatter.name)
                return True
            else:
                logger.debug(
                    "Formatter %s failed on %s: %s",
                    formatter.name,
                    file_path,
                    result.stderr[:200],
                )
                return False
        except (subprocess.TimeoutExpired, FileNotFoundError):
            return False

    def disable_formatter(self, name: str) -> None:
        """Disable a specific formatter."""
        self._disabled_formatters.add(name)

    def enable_formatter(self, name: str) -> None:
        """Enable a previously disabled formatter."""
        self._disabled_formatters.discard(name)

    def get_status(self) -> list[dict]:
        """Get status of all formatters.

        Returns:
            List of dicts with name, available, enabled, extensions.
        """
        self.detect_formatters()
        result = []
        for name, info in (self._available or {}).items():
            result.append(
                {
                    "name": info.name,
                    "available": info.available,
                    "enabled": name not in self._disabled_formatters,
                    "extensions": info.extensions,
                }
            )
        return result
