"""Base types for monitoring utilities in OpenDev."""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any, Mapping


class BaseMonitor(ABC):
    """Interface capturing the lifecycle of monitoring utilities."""

    @abstractmethod
    def start(self, description: str, **kwargs: Any) -> None:
        """Begin monitoring a task."""

    @abstractmethod
    def stop(self) -> Mapping[str, Any]:
        """Stop monitoring and return the collected metrics."""

    @abstractmethod
    def is_running(self) -> bool:
        """Return whether the monitor is currently active."""
