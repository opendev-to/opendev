"""Base classes for tools."""

from abc import ABC, abstractmethod
from typing import Any


class BaseTool(ABC):
    """Base class for all tools."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Tool name."""
        pass

    @property
    @abstractmethod
    def description(self) -> str:
        """Tool description."""
        pass

    @abstractmethod
    def execute(self, **kwargs: Any) -> Any:
        """Execute the tool.

        Args:
            **kwargs: Tool-specific parameters

        Returns:
            Tool execution result
        """
        pass
