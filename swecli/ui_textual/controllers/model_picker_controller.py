"""Model picker controller for the Textual chat app."""

from __future__ import annotations

import inspect
from typing import Any, Mapping, Optional, TYPE_CHECKING

from rich import box
from rich.console import Group, RenderableType
from rich.panel import Panel
from rich.table import Table
from rich.text import Text

from swecli.ui_textual.style_tokens import BLUE_BG_ACTIVE, BLUE_LIGHT, GREY

if TYPE_CHECKING:
    from swecli.ui_textual.chat_app import SWECLIChatApp
    from swecli.ui_textual.managers.interrupt_manager import InterruptManager


class ModelPickerController:
    """Encapsulates the model selection flow rendered inside the conversation log."""

    def __init__(
        self,
        app: "SWECLIChatApp",
        interrupt_manager: Optional["InterruptManager"] = None,
    ) -> None:
        self.app = app
        self._interrupt_manager = interrupt_manager
        self.state: dict[str, Any] | None = None

    # ---------------------------------------------------------------------
    # Lifecycle helpers
    # ---------------------------------------------------------------------

    @property
    def active(self) -> bool:
        return self.state is not None

    def adjust_indices(self, delta: int, first_affected: int) -> None:
        """Adjust panel_start after resize.

        Args:
            delta: Number of lines added (positive) or removed (negative)
            first_affected: First line index affected by the change
        """
        if self.state is None:
            return
        start = self.state.get("panel_start")
        if start is not None and start >= first_affected:
            self.state["panel_start"] = start + delta

    async def start(self) -> None:
        """Begin the model picker flow."""
        if self.active:
            self.app.conversation.add_system_message(
                "Model selector already open — finish the current flow or type X to cancel."
            )
            self.app.refresh()
            return

        try:
            from swecli.config import get_model_registry

            registry = get_model_registry()
        except Exception as exc:  # pragma: no cover
            self.app.conversation.add_system_message(f"❌ Unable to load model registry: {exc}")
            self.app.refresh()
            return

        self.state = {
            "stage": "slot",
            "registry": registry,
            "slot": None,
            "providers": [],
            "provider_index": 0,
            "provider": None,
            "models": [],
            "model_index": 0,
            "slot_items": [],
            "slot_index": 0,
            "panel_start": None,
        }

        # Track state for interrupt handling
        if self._interrupt_manager:
            from swecli.ui_textual.managers.interrupt_manager import InterruptState

            self._interrupt_manager.enter_state(
                InterruptState.MODEL_PICKER,
                controller_ref=self,
            )

        input_field = self.app.input_field
        input_field.load_text("")
        input_field.cursor_position = 0
        input_field.focus()
        self._render_model_slot_panel()

    def end(self, message: str | None, *, clear_panel: bool = False) -> None:
        """Reset picker state and optionally emit a message."""
        # Exit state tracking
        if self._interrupt_manager:
            self._interrupt_manager.exit_state()

        state = self.state
        if clear_panel and state:
            start = state.get("panel_start")
            if start is not None:
                self.app.conversation._truncate_from(start)
        self.state = None
        if message:
            self.app.conversation.add_system_message(message)
        self.app.refresh()

    # ---------------------------------------------------------------------
    # Actions (delegated from ChatApp)
    # ---------------------------------------------------------------------

    def move(self, delta: int) -> None:
        state = self.state
        if not state:
            return

        stage = state.get("stage")
        if stage == "slot":
            items = state.get("slot_items") or []
            if not items:
                return
            index = (state.get("slot_index", 0) + delta) % len(items)
            state["slot_index"] = index
            self._render_model_slot_panel()
            return

        if stage == "provider":
            providers = state.get("providers") or []
            if not providers:
                return
            index = (state.get("provider_index", 0) + delta) % len(providers)
            state["provider_index"] = index
            self._render_provider_panel()
            return

        if stage == "model":
            models = state.get("models") or []
            if not models:
                return
            index = (state.get("model_index", 0) + delta) % len(models)
            state["model_index"] = index
            self._render_model_list_panel()

    def back(self) -> None:
        state = self.state
        if not state:
            return

        stage = state.get("stage")
        if stage == "model":
            state["stage"] = "provider"
            self._render_provider_panel()
        elif stage == "provider":
            state["stage"] = "slot"
            self._render_model_slot_panel()
        else:
            self.cancel()

    def cancel(self) -> None:
        if not self.state:
            return
        self.end(None, clear_panel=True)

    async def confirm(self) -> None:
        state = self.state
        if not state:
            return

        stage = state.get("stage")

        if stage == "slot":
            items = state.get("slot_items") or []
            if not items:
                return
            index = max(0, min(state.get("slot_index", 0), len(items) - 1))
            item = items[index]
            value = item.get("value")
            if value is None:
                return
            state["slot"] = value
            state["stage"] = "provider"
            state["provider_index"] = 0
            state["provider"] = None
            state["models"] = []
            self._render_provider_panel()
            return

        if stage == "provider":
            providers = state.get("providers") or []
            if not providers:
                self.app.conversation.add_system_message(
                    "No providers available — press X to cancel."
                )
                self.app.refresh()
                return
            index = max(0, min(state.get("provider_index", 0), len(providers) - 1))
            entry = providers[index]
            state["provider_index"] = index
            state["provider"] = entry["provider"]
            state["models"] = entry["models"]
            state["model_index"] = 0
            state["stage"] = "model"
            self._render_model_list_panel()
            return

        if stage == "model":
            models = state.get("models") or []
            provider_info = state.get("provider")
            slot = state.get("slot")
            if not models:
                self.app.conversation.add_system_message(
                    "No models to select — press B to go back."
                )
                self.app.refresh()
                return
            if not provider_info or not slot:
                self.app.conversation.add_system_message("Internal model selector state invalid.")
                self.app.refresh()
                return
            index = max(0, min(state.get("model_index", 0), len(models) - 1))
            model_info = models[index]
            # Save immediately and go back to slot menu
            success = await self._commit_single_model(slot, provider_info, model_info)
            state["stage"] = "slot"
            state["provider"] = None
            state["providers"] = []
            state["models"] = []
            state["provider_index"] = 0
            self._render_model_slot_panel()

    async def handle_input(self, raw_value: str) -> bool:
        state = self.state
        if not state:
            return False

        value = (raw_value or "").strip()
        if not value:
            return True

        normalized = value.lower().strip().lstrip("/")
        stage = state.get("stage")

        if stage == "slot":
            if normalized in {"quit"}:
                self.cancel()
                return True

            items = state.get("slot_items") or []
            match_index: int | None = None
            for index, item in enumerate(items):
                tokens = {
                    str(item.get("option", "")).lower(),
                    str(item.get("value", "")).lower(),
                    str(item.get("label", "")).lower(),
                }
                if normalized in tokens:
                    match_index = index
                    break

            if match_index is None:
                self.app.conversation.add_system_message(
                    "Type 1-5 to select a slot, or press Esc to cancel."
                )
                self.app.refresh()
                return True

            self._jump_to(match_index)
            await self.confirm()
            return True

        if stage == "provider":
            if normalized in {"x", "cancel", "quit"}:
                self.cancel()
                return True
            if normalized in {"b", "back"}:
                self.back()
                return True

            providers = state.get("providers") or []
            if not providers:
                self.app.conversation.add_system_message(
                    "No providers available — press X to cancel."
                )
                self.app.refresh()
                return True

            match_index: int | None = None
            if normalized.isdigit():
                candidate = int(normalized) - 1
                if 0 <= candidate < len(providers):
                    match_index = candidate
            else:
                for index, entry in enumerate(providers):
                    provider = entry.get("provider")
                    tokens = {
                        str(getattr(provider, "name", "")).lower(),
                        str(getattr(provider, "id", "")).lower(),
                    }
                    if normalized in tokens:
                        match_index = index
                        break

            if match_index is None:
                self.app.conversation.add_system_message(
                    "Enter a provider number, B to go back, or X to cancel."
                )
                self.app.refresh()
                return True

            self._jump_to(match_index)
            await self.confirm()
            return True

        if stage == "model":
            if normalized in {"x", "cancel", "quit"}:
                self.cancel()
                return True
            if normalized in {"b", "back"}:
                self.back()
                return True

            models = state.get("models") or []
            if not models:
                self.app.conversation.add_system_message(
                    "No models to select — press B to go back."
                )
                self.app.refresh()
                return True

            match_index: int | None = None
            if normalized.isdigit():
                candidate = int(normalized) - 1
                if 0 <= candidate < len(models):
                    match_index = candidate
            else:
                for index, model in enumerate(models):
                    tokens = {
                        str(getattr(model, "name", "")).lower(),
                        str(getattr(model, "id", "")).lower(),
                    }
                    if normalized in tokens:
                        match_index = index
                        break

            if match_index is None:
                self.app.conversation.add_system_message(
                    "Enter a model number, B to go back, or X to cancel."
                )
                self.app.refresh()
                return True

            self._jump_to(match_index)
            await self.confirm()
            return True

        self.end("Model selector reset.", clear_panel=True)
        return True

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _jump_to(self, index: int) -> None:
        state = self.state
        if not state:
            return

        stage = state.get("stage")
        if stage == "slot":
            items = state.get("slot_items") or []
            if not items:
                return
            clamped = max(0, min(index, len(items) - 1))
            state["slot_index"] = clamped
            self._render_model_slot_panel()
            return

        if stage == "provider":
            providers = state.get("providers") or []
            if not providers:
                return
            clamped = max(0, min(index, len(providers) - 1))
            state["provider_index"] = clamped
            self._render_provider_panel()
            return

        if stage == "model":
            models = state.get("models") or []
            if not models:
                return
            clamped = max(0, min(index, len(models) - 1))
            state["model_index"] = clamped
            self._render_model_list_panel()

    def _render_model_slot_panel(self) -> None:
        state = self.state
        if not state:
            return

        config_snapshot = self._get_model_config_snapshot()
        labels = self._model_slot_labels()

        items: list[dict[str, str]] = []
        order = [("normal", "1"), ("thinking", "2"), ("critique", "3"), ("compact", "4"), ("vision", "5")]
        for slot, option in order:
            slot_label = labels.get(slot, slot.title())
            summary = "Not set"
            current = config_snapshot.get(slot, {})
            provider_display = current.get("provider_display") or current.get("provider") or ""
            model_display = current.get("model_display") or current.get("model") or ""
            if provider_display and model_display:
                summary = f"{provider_display}/{model_display}"
            elif provider_display:
                summary = provider_display
            items.append(
                {
                    "value": slot,
                    "label": slot_label,
                    "summary": summary,
                    "option": option,
                }
            )

        state["slot_items"] = items
        index = state.get("slot_index", 0)
        if not 0 <= index < len(items):
            index = 0
        state["slot_index"] = index

        table = Table.grid(expand=False, padding=(0, 1))
        table.add_column(width=2, justify="center")
        table.add_column(width=7, justify="center")
        table.add_column(ratio=1)
        table.add_column(ratio=1)

        for row_index, item in enumerate(items):
            is_active = row_index == index
            pointer = "❯" if is_active else " "
            row_style = f"on {BLUE_BG_ACTIVE}" if is_active else ""
            pointer_style = "bold bright_cyan" if is_active else "dim"
            label_style = "bold white" if is_active else "white"
            summary_style = "dim white" if is_active else "dim"
            option_style = "bold bright_cyan" if is_active else "dim"
            table.add_row(
                Text(pointer, style=pointer_style),
                Text(item["option"], style=option_style),
                Text(item["label"], style=label_style),
                Text(item["summary"], style=summary_style),
                style=row_style,
            )

        instructions = Text(
            "Use ↑/↓ or 1-5 to select a slot, Enter to configure, Esc to cancel.",
            style=f"italic {GREY}",
        )
        header = Text("Select which model slot you'd like to configure.", style=BLUE_LIGHT)
        panel = Panel(
            Group(header, table, instructions),
            title="[bold]Model Configuration[/bold]",
            title_align="left",
            border_style="bright_cyan",
            padding=(1, 2),
        )
        self._post_model_panel(panel)

    def _render_provider_panel(self) -> None:
        state = self.state
        if not state:
            return

        slot = state.get("slot")
        registry = state.get("registry")
        providers = self._compute_providers_for_slot(slot, registry)

        if not providers:
            labels = self._model_slot_labels()
            self.app.conversation.add_system_message(
                f"⚠️ No providers currently offer models for {labels.get(slot, slot)}."
            )
            state["stage"] = "slot"
            state["providers"] = []
            self._render_model_slot_panel()
            return

        state["providers"] = providers
        index = state.get("provider_index", 0)
        if not 0 <= index < len(providers):
            index = 0
        state["provider_index"] = index

        labels = self._model_slot_labels()
        description = self._model_slot_description(slot or "")

        max_rows = 7
        total = len(providers)
        half_window = max_rows // 2
        start = max(0, index - half_window)
        end = start + max_rows
        if end > total:
            end = total
            start = max(0, end - max_rows)

        visible = providers[start:end]
        before_hidden = start > 0
        after_hidden = end < total

        table = Table.grid(expand=False, padding=(0, 1))
        table.add_column(width=2, justify="center")
        table.add_column(width=7, justify="center")
        table.add_column(ratio=1)
        table.add_column(ratio=1)

        if before_hidden:
            table.add_row(
                Text(" ", style="dim"),
                Text("…", style="dim"),
                Text(f"{start} more above", style="dim"),
                Text("", style="dim"),
            )

        for offset, entry in enumerate(visible):
            row_index = start + offset
            provider = entry["provider"]
            models = entry["models"]
            total_models = len(models)
            max_context = max((model.context_length for model in models), default=0)
            context_display = (
                f"{max_context // 1000}k context"
                if max_context >= 1000
                else f"{max_context} context"
            )
            summary_text = f"{total_models} models · {context_display}"

            is_active = row_index == index
            pointer = "❯" if is_active else " "
            row_style = f"on {BLUE_BG_ACTIVE}" if is_active else ""
            pointer_style = "bold bright_cyan" if is_active else "dim"
            label_style = "bold white" if is_active else "white"
            option_style = "bold bright_cyan" if is_active else "dim"
            summary_style = "dim white" if is_active else "dim"
            table.add_row(
                Text(pointer, style=pointer_style),
                Text(str(row_index + 1), style=option_style),
                Text(provider.name, style=label_style),
                Text(summary_text, style=summary_style),
                style=row_style,
            )

        if after_hidden:
            remaining = total - end
            table.add_row(
                Text(" ", style="dim"),
                Text("…", style="dim"),
                Text(f"{remaining} more below", style="dim"),
                Text("", style="dim"),
            )

        instructions = Text(
            "Use ↑/↓ or number keys, Enter to view models, B to go back, Esc to cancel.",
            style=f"italic {GREY}",
        )
        subtitle = Text(
            f"{labels.get(slot, slot.title())} · {description}",
            style=BLUE_LIGHT,
        )
        panel = Panel(
            Group(subtitle, table, instructions),
            title="[bold]Choose a Provider[/bold]",
            title_align="left",
            border_style="bright_blue",
            padding=(1, 2),
        )
        self._post_model_panel(panel)

    def _render_model_list_panel(self) -> None:
        state = self.state
        if not state:
            return

        provider_entry = state.get("provider")
        models = state.get("models") or []
        slot = state.get("slot")

        if not provider_entry or not models:
            self.app.conversation.add_system_message(
                "No models available for the selected provider."
            )
            state["stage"] = "provider"
            self._render_provider_panel()
            return

        labels = self._model_slot_labels()

        index = state.get("model_index", 0)
        if not 0 <= index < len(models):
            index = 0
        state["model_index"] = index

        table = Table.grid(expand=False, padding=(0, 1))
        table.add_column(width=2, justify="center")
        table.add_column(width=7, justify="center")
        table.add_column(ratio=1)
        table.add_column(width=14, justify="right")

        for row_index, model in enumerate(models):
            model_name = model.name
            if model.context_length >= 1000:
                context_k = f"{model.context_length // 1000}k context"
            else:
                context_k = f"{model.context_length} context"
            is_active = row_index == index
            pointer = "❯" if is_active else " "
            row_style = f"on {BLUE_BG_ACTIVE}" if is_active else ""
            pointer_style = "bold bright_cyan" if is_active else "dim"
            label_style = "bold white" if is_active else "white"
            info_style = "dim white" if is_active else "dim"
            option_style = "bold bright_cyan" if is_active else "dim"
            table.add_row(
                Text(pointer, style=pointer_style),
                Text(str(row_index + 1), style=option_style),
                Text(model_name, style=label_style),
                Text(context_k, style=info_style),
                style=row_style,
            )

        instructions = Text(
            "Use ↑/↓ or number keys, Enter to apply, B to go back, Esc to cancel.",
            style=f"italic {GREY}",
        )
        subtitle = Text(
            f"{provider_entry.name} · {labels.get(slot, slot.title())}",
            style=BLUE_LIGHT,
        )
        panel = Panel(
            Group(subtitle, table, instructions),
            title="[bold]Select a Model[/bold]",
            title_align="left",
            border_style="bright_green",
            padding=(1, 2),
        )
        self._post_model_panel(panel)

    def _render_model_summary(self) -> None:
        snapshot = self._get_model_config_snapshot()
        labels = self._model_slot_labels()

        table = Table(
            show_header=True,
            header_style=f"bold {BLUE_LIGHT}",
            box=box.ROUNDED,
            expand=True,
        )
        table.add_column("Slot", style="bold white")
        table.add_column("Provider", style=GREY)
        table.add_column("Model", style=GREY)

        for slot in ["normal", "thinking", "critique", "compact", "vision"]:
            entry = snapshot.get(slot, {})
            provider_display = entry.get("provider_display") or entry.get("provider") or "Not set"
            model_display = entry.get("model_display") or entry.get("model") or "—"
            if provider_display == "Not set":
                model_display = "—"
            table.add_row(labels.get(slot, slot.title()), provider_display, model_display)

        instructions = Text(
            "Type /models again to reopen the selector anytime.", style=f"italic {GREY}"
        )
        panel = Panel(
            Group(Text("Current model configuration", style=BLUE_LIGHT), table, instructions),
            title="[bold]Model Summary[/bold]",
            title_align="left",
            border_style="bright_cyan",
            padding=(1, 2),
        )
        self._post_model_panel(panel)

    def _compute_providers_for_slot(self, slot: str, registry) -> list[dict[str, Any]]:
        if registry is None:
            return []

        capability_map = {
            "normal": None,
            "thinking": "reasoning",
            "critique": None,
            "compact": None,
            "vision": "vision",
        }
        required_capability = capability_map.get(slot)
        universal_providers = {"openai", "anthropic"}

        providers: list[dict[str, Any]] = []
        for provider in registry.list_providers():
            is_universal = provider.id in universal_providers
            if slot == "normal":
                models = provider.list_models()
            else:
                if is_universal:
                    models = provider.list_models()
                else:
                    models = [
                        model
                        for model in provider.list_models()
                        if required_capability and required_capability in model.capabilities
                    ]
            if not models:
                continue

            models = sorted(models, key=lambda m: m.context_length, reverse=True)
            providers.append(
                {
                    "provider": provider,
                    "models": models,
                    "is_universal": is_universal,
                }
            )

        return providers

    def _get_model_config_snapshot(self) -> dict[str, dict[str, str]]:
        snapshot: dict[str, dict[str, str]] = {}
        get_model_config = self.app.get_model_config
        if get_model_config:
            try:
                raw_config = get_model_config()
                if isinstance(raw_config, Mapping):
                    for slot, value in raw_config.items():
                        if isinstance(value, Mapping):
                            snapshot[slot] = {
                                "provider": str(value.get("provider", "") or ""),
                                "provider_display": str(
                                    value.get("provider_display", "")
                                    or value.get("provider", "")
                                    or ""
                                ),
                                "model": str(value.get("model", "") or ""),
                                "model_display": str(
                                    value.get("model_display", "") or value.get("model", "") or ""
                                ),
                            }
            except Exception:  # pragma: no cover
                snapshot = {}

        if not snapshot and self.app.model_slots:
            for slot, (provider_display, model_display) in self.app.model_slots.items():
                snapshot[slot] = {
                    "provider_display": provider_display,
                    "model_display": model_display,
                }

        return snapshot

    def _post_model_panel(self, panel: RenderableType) -> None:
        state = self.state
        if state is not None:
            start = state.get("panel_start")
            conversation = self.app.conversation
            if start is None or start > len(conversation.lines):
                state["panel_start"] = len(conversation.lines)
            else:
                conversation._truncate_from(start)
        conversation.write(panel)
        conversation.write(Text(""))
        conversation.scroll_end(animate=False)
        self.app.refresh()

    async def _commit_single_model(self, slot: str, provider_info, model_info) -> bool:
        """Save a single model selection immediately.

        Returns:
            True if saved successfully, False otherwise.
        """
        if not self.app.on_model_selected:
            self.app.conversation.add_system_message("No handler available to update models.")
            return False

        labels = self._model_slot_labels()
        display_name = f"{provider_info.name}/{model_info.name}"

        try:
            result = self.app.on_model_selected(slot, provider_info.id, model_info.id)
            if inspect.isawaitable(result):
                result = await result
        except Exception as exc:  # pragma: no cover
            self.app.conversation.add_error(
                f"{labels.get(slot, slot.title())} model not saved: Exception while saving: {exc}"
            )
            return False

        if getattr(result, "success", None):
            message = getattr(result, "message", "") or ""
            summary = display_name
            if message:
                summary = f"{summary} — {message}"
            self.app.conversation.add_system_message(
                f"✓ {labels.get(slot, slot.title())} model saved: {summary}"
            )

            # Auto-populate thinking/vision slots when setting normal model
            if slot == "normal":
                await self._auto_populate_slots(provider_info, model_info, labels)

            return True
        else:
            message = getattr(result, "message", None) or "Model update failed."
            self.app.conversation.add_error(
                f"{labels.get(slot, slot.title())} model not saved: {message}"
            )
            return False

    async def _auto_populate_slots(self, provider_info, model_info, labels: dict[str, str]) -> None:
        """Auto-populate thinking/vision slots based on model capabilities.

        Only populates slots that are not already set.
        """
        # Get current config to check if slots are set
        config_snapshot = self._get_model_config_snapshot()
        thinking_set = config_snapshot.get("thinking", {}).get("model")
        vision_set = config_snapshot.get("vision", {}).get("model")

        display_name = f"{provider_info.name}/{model_info.name}"
        capabilities = getattr(model_info, "capabilities", []) or []

        # Auto-populate thinking slot if model has reasoning capability AND not set
        if "reasoning" in capabilities and not thinking_set:
            try:
                result = self.app.on_model_selected("thinking", provider_info.id, model_info.id)
                if inspect.isawaitable(result):
                    result = await result
                if getattr(result, "success", None):
                    self.app.conversation.add_system_message(
                        f"  ⎿  {labels.get('thinking', 'Thinking')} auto-set: {display_name}"
                    )
            except Exception:
                pass  # Silently skip if auto-populate fails

        # Auto-populate vision slot if model has vision capability AND not set
        if "vision" in capabilities and not vision_set:
            try:
                result = self.app.on_model_selected("vision", provider_info.id, model_info.id)
                if inspect.isawaitable(result):
                    result = await result
                if getattr(result, "success", None):
                    self.app.conversation.add_system_message(
                        f"  ⎿  {labels.get('vision', 'Vision')} auto-set: {display_name}"
                    )
            except Exception:
                pass  # Silently skip if auto-populate fails

        # Auto-populate compact slot (any model can do summarization) AND not set
        compact_set = config_snapshot.get("compact", {}).get("model")
        if not compact_set:
            try:
                result = self.app.on_model_selected("compact", provider_info.id, model_info.id)
                if inspect.isawaitable(result):
                    result = await result
                if getattr(result, "success", None):
                    self.app.conversation.add_system_message(
                        f"  ⎿  {labels.get('compact', 'Compact')} auto-set: {display_name}"
                    )
            except Exception:
                pass  # Silently skip if auto-populate fails

    @staticmethod
    def _model_slot_labels() -> dict[str, str]:
        return {
            "normal": "Normal (Primary)",
            "thinking": "Thinking (Reasoning)",
            "critique": "Critique (Self-Critique)",
            "compact": "Compact (Summarization)",
            "vision": "Vision (Multimodal)",
        }

    @staticmethod
    def _model_slot_description(slot: str) -> str:
        descriptions = {
            "normal": "Standard coding and chat tasks",
            "thinking": "Deep reasoning and planning",
            "critique": "Self-critique of reasoning outputs",
            "compact": "Context compaction summaries",
            "vision": "Image analysis and multimodal inputs",
        }
        return descriptions.get(slot, "")
