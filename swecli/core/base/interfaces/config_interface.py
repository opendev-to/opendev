"""Configuration manager interface used by the core system."""

from __future__ import annotations

from pathlib import Path
from typing import Protocol, Sequence

from swecli.models.config import AppConfig


class ConfigManagerInterface(Protocol):
    """Operations required from a configuration manager implementation."""

    working_dir: Path

    def load_config(self) -> AppConfig:
        """Load configuration from disk and return it."""

    def get_config(self) -> AppConfig:
        """Return the cached configuration, loading when necessary."""

    def save_config(self, config: AppConfig, *, global_config: bool = False) -> None:
        """Persist the provided configuration to disk."""

    def ensure_directories(self) -> None:
        """Create any directories required by the current configuration."""

    def load_context_files(self) -> Sequence[str]:
        """Load contextual documentation files for the active workspace."""
