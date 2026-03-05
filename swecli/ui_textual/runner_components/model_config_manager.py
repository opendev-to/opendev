"""Model configuration management for TextualRunner.

This module handles model configuration, switching, and UI updates for model slots.
"""

from __future__ import annotations

import asyncio
from typing import Any, Optional

from swecli.core.runtime import ConfigManager
from swecli.repl.repl import REPL


class ModelConfigManager:
    """Manages model configuration, selection, and UI updates."""

    def __init__(
        self,
        config_manager: ConfigManager,
        repl: REPL,
    ) -> None:
        """Initialize the manager.
        
        Args:
            config_manager: Configuration manager instance.
            repl: REPL instance for agent rebuilding and commands.
        """
        self._config_manager = config_manager
        self._repl = repl
        self._app: Any | None = None

    def set_app(self, app: Any) -> None:
        """Set the Textual app instance for UI updates."""
        self._app = app

    def get_model_config_snapshot(self) -> dict[str, dict[str, str]]:
        """Return current model configuration details for the UI."""
        config = self._config_manager.get_config()

        try:
            from swecli.config import get_model_registry

            registry = get_model_registry()
        except Exception:  # pragma: no cover - defensive
            registry = None

        def resolve(
            provider_id: Optional[str], model_id: Optional[str]
        ) -> dict[str, str]:
            if not provider_id or not model_id:
                return {}

            provider_display = provider_id.capitalize()
            model_display = model_id

            if registry is not None:
                provider_info = registry.get_provider(provider_id)
                if provider_info:
                    provider_display = provider_info.name
                found = registry.find_model_by_id(model_id)
                if found:
                    _, _, model_info = found
                    model_display = model_info.name
            else:
                if "/" in model_id:
                    model_display = model_id.split("/")[-1]

            return {
                "provider": provider_id,
                "provider_display": provider_display,
                "model": model_id,
                "model_display": model_display,
            }

        snapshot: dict[str, dict[str, str]] = {}
        snapshot["normal"] = resolve(config.model_provider, config.model)

        thinking_entry = resolve(
            config.model_thinking_provider, config.model_thinking
        )
        if thinking_entry:
            snapshot["thinking"] = thinking_entry

        vision_entry = resolve(config.model_vlm_provider, config.model_vlm)
        if vision_entry:
            snapshot["vision"] = vision_entry

        critique_entry = resolve(config.model_critique_provider, config.model_critique)
        if critique_entry:
            snapshot["critique"] = critique_entry

        compact_entry = resolve(config.model_compact_provider, config.model_compact)
        if compact_entry:
            snapshot["compact"] = compact_entry

        return snapshot

    def refresh_ui_config(self) -> None:
        """Refresh cached config-driven UI indicators after config changes."""
        if not self._app:
            return

        # Use get_model_config_snapshot as single source of truth for display names
        snapshot = self.get_model_config_snapshot()
        normal_info = snapshot.get("normal", {})
        provider_display = normal_info.get("provider_display", "")
        model_display_name = normal_info.get("model_display", "")
        model_display = f"{provider_display}/{model_display_name}" if provider_display else model_display_name

        if hasattr(self._app, "update_primary_model"):
            self._app.update_primary_model(model_display)
        if hasattr(self._app, "update_model_slots"):
            self._app.update_model_slots(self._build_model_slots(snapshot))

    async def apply_model_selection(
        self, slot: str, provider_id: str, model_id: str
    ) -> Any:
        """Apply a model selection coming from the Textual UI."""
        # This calls internal repl methods.
        # We assume repl.config_commands exists and has _switch_to_model
        if not hasattr(self._repl, "config_commands"):
             # Fallback if config_commands not available (e.g. dummy repl)
             from types import SimpleNamespace
             return SimpleNamespace(success=False, message="Config commands not available")

        result = await asyncio.to_thread(
            self._repl.config_commands._switch_to_model,
            provider_id,
            model_id,
            slot,
        )
        if result.success:
            # Rebuild agents with new config (needed for API key changes)
            await asyncio.to_thread(self._repl.rebuild_agents)
            self.refresh_ui_config()
        return result

    def _build_model_slots(
        self, snapshot: Optional[dict[str, dict[str, str]]] = None
    ) -> dict[str, tuple[str, str]]:
        """Prepare formatted model slot information for the footer.

        Args:
            snapshot: Optional pre-computed snapshot from get_model_config_snapshot().
                      If not provided, will compute it.
        """
        if snapshot is None:
            snapshot = self.get_model_config_snapshot()

        def extract_slot(slot_name: str) -> tuple[str, str] | None:
            info = snapshot.get(slot_name, {})
            if not info:
                return None
            provider_display = info.get("provider_display", "")
            model_display = info.get("model_display", "")
            if not provider_display or not model_display:
                return None
            return (provider_display, model_display)

        slots = {}

        normal = extract_slot("normal")
        if normal:
            slots["normal"] = normal

        # Show thinking slot if explicitly set (even if same as normal)
        thinking = extract_slot("thinking")
        if thinking:
            slots["thinking"] = thinking

        # Show vision slot if explicitly set (even if same as normal)
        vision = extract_slot("vision")
        if vision:
            slots["vision"] = vision

        # Show critique slot if explicitly set
        critique = extract_slot("critique")
        if critique:
            slots["critique"] = critique

        # Show compact slot if explicitly set
        compact = extract_slot("compact")
        if compact:
            slots["compact"] = compact

        return slots
