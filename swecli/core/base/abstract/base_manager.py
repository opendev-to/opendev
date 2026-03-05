"""Generic base class for stateful managers used across the core."""

from __future__ import annotations

from abc import ABC
from typing import Any


class BaseManager(ABC):
    """Provides shared console-aware logging helpers for managers."""

    def __init__(self, *, console: Union[Any, None] = None) -> None:
        self.console = console

    def _log(self, message: str) -> None:
        """Emit a message to the configured console if available."""
        if self.console is not None:
            try:
                self.console.print(message)
            except Exception:
                # Fallback when the console implementation does not expose print
                pass
