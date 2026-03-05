"""Abstract foundation for executable tools in OpenDev."""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any, Mapping


class BaseTool(ABC):
    """Minimal contract for tools invoked by the agent runtime."""

    name: str
    description: str

    @abstractmethod
    def run(self, **kwargs: Any) -> Mapping[str, Any]:
        """Execute the tool and return a structured result."""
