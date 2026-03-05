"""Custom TextArea widget used by the Textual chat app."""

from __future__ import annotations

import asyncio
import inspect
from dataclasses import dataclass
from typing import Any, Optional

from prompt_toolkit.completion import Completer
from prompt_toolkit.document import Document as PTDocument
from textual.events import Key, MouseDown, MouseMove, MouseUp, Paste
from textual.message import Message
from textual.widgets import TextArea


class ChatTextArea(TextArea):
    """Multi-line text area that sends on Enter and inserts newline on Shift+Enter."""

    # Debounce delay for autocomplete in seconds
    AUTOCOMPLETE_DEBOUNCE_MS = 100

    def __init__(
        self,
        *args,
        paste_threshold: int = 200,
        completer: Optional[Completer] = None,
        **kwargs,
    ) -> None:
        self._completer: Optional[Completer] = completer
        self._completions: list[Any] = []
        self._completion_entries: list[tuple[str, str]] = []
        self._highlight_index: int | None = None
        super().__init__(*args, **kwargs)
        self._paste_threshold = paste_threshold
        self._paste_counter = 0
        self._large_paste_cache: dict[str, str] = {}
        self._suppress_next_autocomplete = False
        self._autocomplete_timer = None
        self._last_autocomplete_text: str = ""

    @dataclass
    class Submitted(Message):
        """Message emitted when the user submits the chat input."""

        text_area: "ChatTextArea"
        value: str

        @property
        def control(self) -> "ChatTextArea":
            """Compatibility alias matching Textual input events."""
            return self.text_area

    def set_completer(self, completer: Optional[Completer]) -> None:
        """Assign a prompt-toolkit completer for @ mentions and / commands."""

        self._completer = completer
        self.update_suggestion()

    def update_suggestion(self) -> None:
        """Populate the inline suggestion using the configured completer.

        Uses debouncing to avoid blocking the UI on every keystroke.
        """
        super().update_suggestion()

        if getattr(self, "_suppress_next_autocomplete", False):
            self._suppress_next_autocomplete = False
            self._clear_completions()
            return

        if not self._completer:
            self._clear_completions()
            return

        if self.selection.start != self.selection.end:
            self._clear_completions()
            return

        text = self.text or ""
        if not text:
            self._clear_completions()
            return

        # Skip if text hasn't changed (avoid redundant work)
        if text == self._last_autocomplete_text:
            return

        # Cancel any pending autocomplete timer
        if self._autocomplete_timer is not None:
            self._autocomplete_timer.stop()
            self._autocomplete_timer = None

        # Debounce: schedule autocomplete to run after a short delay
        self._autocomplete_timer = self.set_timer(
            self.AUTOCOMPLETE_DEBOUNCE_MS / 1000.0,
            self._do_autocomplete,
        )

    def _do_autocomplete(self) -> None:
        """Actually perform the autocomplete lookup (called after debounce)."""
        self._autocomplete_timer = None

        text = self.text or ""
        if not text:
            self._clear_completions()
            return

        self._last_autocomplete_text = text

        try:
            cursor_index = self.document.get_index_from_location(self.cursor_location)
        except Exception:
            self._clear_completions()
            return

        document = PTDocument(text=text, cursor_position=cursor_index)
        try:
            completions = list(self._completer.get_completions(document, None))
        except Exception:
            self._clear_completions()
            return

        self._completions = completions
        self._completion_entries = [
            (
                getattr(item, "display_text", item.text),
                getattr(item, "display_meta_text", "") or "",
            )
            for item in completions
        ]

        if self._completions:
            self._set_highlight_index(0)
        else:
            self._set_highlight_index(None)

        self._notify_autocomplete()

    def _notify_autocomplete(self) -> None:
        """Inform the parent app about available autocomplete entries."""

        try:
            app = self.app  # type: ignore[attr-defined]
        except Exception:
            app = None

        if app and hasattr(app, "update_autocomplete"):
            try:
                selected = self._highlight_index if self._highlight_index is not None else None
                app.update_autocomplete(self._completion_entries, selected)
            except Exception:
                pass

    def _clear_completions(self) -> None:
        """Reset completion tracking and hide popup."""

        self._completions = []
        self._completion_entries = []
        self._highlight_index = None
        self.suggestion = ""
        # Notify app to hide popup
        try:
            app = self.app  # type: ignore[attr-defined]
            if app and hasattr(app, "update_autocomplete"):
                app.update_autocomplete([], None)
        except Exception:
            pass

    def _dismiss_autocomplete(self) -> None:
        """Hide autocomplete suggestions and notify parent app."""

        if not self._completions and not self.suggestion:
            return

        self._clear_completions()
        try:
            app = self.app  # type: ignore[attr-defined]
        except Exception:
            app = None

        if app and hasattr(app, "update_autocomplete"):
            try:
                app.update_autocomplete([], None)
            except Exception:
                pass
        self.suggestion = ""
        self._notify_autocomplete()

    def _set_highlight_index(self, index: int | None) -> None:
        """Update active selection and inline suggestion."""

        if not self._completions or index is None:
            self._highlight_index = None
            self.suggestion = ""
            return

        clamped = max(0, min(index, len(self._completions) - 1))
        self._highlight_index = clamped
        details = self._compute_completion_details(self._completions[clamped])
        if details:
            self.suggestion = details["remainder"]
        else:
            self.suggestion = ""

    def _compute_completion_details(self, completion: Any) -> dict[str, Any] | None:
        """Compute replacement metadata for the given completion."""

        try:
            cursor_index = self.document.get_index_from_location(self.cursor_location)
        except Exception:
            return None

        text = self.text or ""
        start_pos = getattr(completion, "start_position", 0) or 0
        replace_start = max(0, cursor_index + start_pos)
        replace_end = cursor_index
        if replace_start > replace_end:
            replace_start = replace_end

        existing = text[replace_start:replace_end]
        completion_text = getattr(completion, "text", "") or ""
        if existing and not completion_text.startswith(existing):
            return None

        remainder = completion_text[len(existing) :] if existing else completion_text
        return {
            "remainder": remainder,
            "replace_start": replace_start,
            "replace_end": replace_end,
            "completion_text": completion_text,
        }

    def _move_completion_selection(self, delta: int) -> None:
        """Move the highlighted completion entry up or down."""

        if not self._completions:
            return

        current = self._highlight_index or 0
        new_index = (current + delta) % len(self._completions)
        self._set_highlight_index(new_index)
        self._notify_autocomplete()

    def _accept_completion_selection(self) -> bool:
        """Apply the currently selected completion into the text area."""

        if not self._completions:
            return False

        index = self._highlight_index or 0
        index = max(0, min(index, len(self._completions) - 1))
        completion = self._completions[index]
        details = self._compute_completion_details(completion)
        if not details:
            return False

        completion_text = details["completion_text"]
        replace_start = details["replace_start"]
        replace_end = details["replace_end"]

        start_location = self.document.get_location_from_index(replace_start)
        end_location = self.document.get_location_from_index(replace_end)

        # Set flag BEFORE text modification to catch any triggered events
        self._suppress_next_autocomplete = True

        # Cancel any pending autocomplete timer
        if self._autocomplete_timer is not None:
            self._autocomplete_timer.stop()
            self._autocomplete_timer = None

        result = self._replace_via_keyboard(completion_text, start_location, end_location)
        if result is not None:
            self.cursor_location = result.end_location

        # Update last text to prevent "text changed" triggers from re-running autocomplete
        self._last_autocomplete_text = self.text or ""

        self._clear_completions()
        self.update_suggestion()
        return True

    async def _on_key(self, event: Key) -> None:
        """Intercept Enter to submit while preserving Shift+Enter for new lines."""
        app = getattr(self, "app", None)

        # Cancel exit confirmation on any key except Ctrl+C
        if hasattr(app, "_exit_confirmation_mode") and app._exit_confirmation_mode:
            if event.key != "ctrl+c":
                if hasattr(app, "_cancel_exit_confirmation"):
                    app._cancel_exit_confirmation()

        # Handle ESC key for interrupt
        if event.key == "escape":
            # First check autocomplete - highest priority (fast path, avoids action_interrupt overhead)
            if self._completions:
                event.stop()
                event.prevent_default()
                self._dismiss_autocomplete()
                return

            # Delegate everything else to the centralized action_interrupt handler
            if hasattr(app, "action_interrupt"):
                event.stop()
                event.prevent_default()
                app.action_interrupt()
                return

        approval_controller = getattr(app, "_approval_controller", None)
        approval_mode = bool(approval_controller and getattr(approval_controller, "active", False))

        if approval_mode:
            if event.key == "up":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_approval_move"):
                    app._approval_move(-1)
                return
            if event.key == "down":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_approval_move"):
                    app._approval_move(1)
                return
            # ESC/Ctrl+C already handled by InterruptManager above
            if event.key in {"enter", "return"} and "+" not in event.key:
                event.stop()
                event.prevent_default()
                if hasattr(app, "_approval_confirm"):
                    app._approval_confirm()
                return

        # Plan approval prompt keyboard handling
        plan_approval_controller = getattr(app, "_plan_approval_controller", None)
        plan_approval_mode = bool(
            plan_approval_controller and getattr(plan_approval_controller, "active", False)
        )

        if plan_approval_mode:
            if event.key == "up":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_plan_approval_move"):
                    app._plan_approval_move(-1)
                return
            if event.key == "down":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_plan_approval_move"):
                    app._plan_approval_move(1)
                return
            # ESC/Ctrl+C already handled by InterruptManager above
            if event.key in {"enter", "return"} and "+" not in event.key:
                event.stop()
                event.prevent_default()
                if hasattr(app, "_plan_approval_confirm"):
                    app._plan_approval_confirm()
                return

        # Ask-user prompt keyboard handling
        ask_user_controller = getattr(app, "_ask_user_controller", None)
        ask_user_mode = bool(ask_user_controller and getattr(ask_user_controller, "active", False))
        ask_user_other_mode = bool(
            ask_user_controller and getattr(ask_user_controller, "_other_mode", False)
        )

        if ask_user_mode:
            # In "Other" mode, allow typing but still handle Enter
            # ESC/Ctrl+C already handled by InterruptManager above
            if ask_user_other_mode:
                if event.key in {"enter", "return"} and "+" not in event.key:
                    event.stop()
                    event.prevent_default()
                    if hasattr(app, "_ask_user_confirm"):
                        app._ask_user_confirm()
                    return
                # Allow other keys (typing) to pass through
                await super()._on_key(event)
                self.update_suggestion()
                # Update live preview in the ask-user panel
                if hasattr(ask_user_controller, "update_input_preview"):
                    ask_user_controller.update_input_preview(self.text or "")
                return

            # Normal ask-user mode - intercept navigation keys
            if event.key == "up":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_ask_user_move"):
                    app._ask_user_move(-1)
                return
            if event.key == "down":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_ask_user_move"):
                    app._ask_user_move(1)
                return
            if event.key == "left":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_ask_user_go_back"):
                    app._ask_user_go_back()
                return
            if event.key == "right":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_ask_user_go_forward"):
                    app._ask_user_go_forward()
                return
            if event.key in {" ", "space"}:
                # Space toggles multi-select
                event.stop()
                event.prevent_default()
                if hasattr(app, "_ask_user_toggle"):
                    app._ask_user_toggle()
                return
            # ESC/Ctrl+C already handled by InterruptManager above
            if event.key in {"enter", "return"} and "+" not in event.key:
                event.stop()
                event.prevent_default()
                if hasattr(app, "_ask_user_confirm"):
                    app._ask_user_confirm()
                return

        model_picker = getattr(app, "_model_picker", None)
        picker_active = bool(model_picker and getattr(model_picker, "active", False))

        if picker_active:
            if event.key == "up":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_model_picker_move"):
                    app._model_picker_move(-1)
                return
            if event.key == "down":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_model_picker_move"):
                    app._model_picker_move(1)
                return
            if event.key in {"enter", "return"} and "+" not in event.key:
                event.stop()
                event.prevent_default()
                confirm = getattr(app, "_model_picker_confirm", None)
                if confirm is not None:
                    result = confirm()
                    if inspect.isawaitable(result):
                        asyncio.create_task(result)
                return
            # ESC/Ctrl+C already handled by InterruptManager above
            if event.character and event.character.lower() == "b":
                event.stop()
                event.prevent_default()
                if hasattr(app, "_model_picker_back"):
                    app._model_picker_back()
                return

        # Agent creator wizard keyboard handling
        agent_creator = getattr(app, "_agent_creator", None)
        wizard_stage = None
        if agent_creator and hasattr(agent_creator, "state") and agent_creator.state:
            wizard_stage = agent_creator.state.get("stage")

        # During GENERATING stage, allow normal input so user can queue messages
        agent_wizard_active = bool(
            agent_creator
            and getattr(agent_creator, "active", False)
            and wizard_stage != "generating"
        )

        if agent_wizard_active:

            text_input_stages = ("identifier", "prompt", "description")
            is_text_input_stage = wizard_stage in text_input_stages

            # SELECTION STAGES: intercept up/down for navigation, B for back
            if not is_text_input_stage:
                if event.key == "up":
                    event.stop()
                    event.prevent_default()
                    if hasattr(app, "_agent_wizard_move"):
                        app._agent_wizard_move(-1)
                    return
                if event.key == "down":
                    event.stop()
                    event.prevent_default()
                    if hasattr(app, "_agent_wizard_move"):
                        app._agent_wizard_move(1)
                    return
                if event.character and event.character.lower() == "b":
                    event.stop()
                    event.prevent_default()
                    if hasattr(app, "_agent_wizard_back"):
                        app._agent_wizard_back()
                    return

                # TOOLS STAGE: Space key toggles selection
                if wizard_stage == "tools" and event.key == " ":
                    event.stop()
                    event.prevent_default()
                    if hasattr(agent_creator, "toggle_tool_selection"):
                        agent_creator.toggle_tool_selection()
                    return

            # BOTH STAGES: Enter confirms
            # ESC/Ctrl+C already handled by InterruptManager above
            if event.key in {"enter", "return"} and "+" not in event.key:
                event.stop()
                event.prevent_default()
                # Sync current text to state before confirming (for text input stages)
                if is_text_input_stage and hasattr(agent_creator, "update_input_preview"):
                    agent_creator.update_input_preview(self.text or "")
                confirm = getattr(app, "_agent_wizard_confirm", None)
                if confirm is not None:
                    result = confirm()
                    if inspect.isawaitable(result):
                        asyncio.create_task(result)
                return

            # TEXT INPUT STAGES: Allow typing, update live preview after keystroke
            if is_text_input_stage:
                await super()._on_key(event)
                self.update_suggestion()
                # Update live preview in the wizard panel
                if hasattr(agent_creator, "update_input_preview"):
                    agent_creator.update_input_preview(self.text or "")
                return

        # Skill creator wizard keyboard handling
        skill_creator = getattr(app, "_skill_creator", None)
        skill_wizard_stage = None
        if skill_creator and hasattr(skill_creator, "state") and skill_creator.state:
            skill_wizard_stage = skill_creator.state.get("stage")

        # During GENERATING stage, allow normal input so user can queue messages
        skill_wizard_active = bool(
            skill_creator
            and getattr(skill_creator, "active", False)
            and skill_wizard_stage != "generating"
        )

        if skill_wizard_active:

            skill_text_input_stages = ("identifier", "purpose", "description")
            is_skill_text_input_stage = skill_wizard_stage in skill_text_input_stages

            # SELECTION STAGES: intercept up/down for navigation, B for back
            if not is_skill_text_input_stage:
                if event.key == "up":
                    event.stop()
                    event.prevent_default()
                    if hasattr(app, "_skill_wizard_move"):
                        app._skill_wizard_move(-1)
                    return
                if event.key == "down":
                    event.stop()
                    event.prevent_default()
                    if hasattr(app, "_skill_wizard_move"):
                        app._skill_wizard_move(1)
                    return
                if event.character and event.character.lower() == "b":
                    event.stop()
                    event.prevent_default()
                    if hasattr(app, "_skill_wizard_back"):
                        app._skill_wizard_back()
                    return

            # BOTH STAGES: Enter confirms
            # ESC/Ctrl+C already handled by InterruptManager above
            if event.key in {"enter", "return"} and "+" not in event.key:
                event.stop()
                event.prevent_default()
                # Sync current text to state before confirming (for text input stages)
                if is_skill_text_input_stage and hasattr(skill_creator, "update_input_preview"):
                    skill_creator.update_input_preview(self.text or "")
                confirm = getattr(app, "_skill_wizard_confirm", None)
                if confirm is not None:
                    result = confirm()
                    if inspect.isawaitable(result):
                        asyncio.create_task(result)
                return

            # TEXT INPUT STAGES: Allow typing, update live preview after keystroke
            if is_skill_text_input_stage:
                await super()._on_key(event)
                self.update_suggestion()
                # Update live preview in the wizard panel
                if hasattr(skill_creator, "update_input_preview"):
                    skill_creator.update_input_preview(self.text or "")
                return

        if event.key in {"pageup", "pagedown"}:
            event.stop()
            event.prevent_default()
            if hasattr(app, "action_scroll_up") and event.key == "pageup":
                app.action_scroll_up()
            elif hasattr(app, "action_scroll_down") and event.key == "pagedown":
                app.action_scroll_down()
            return

        if self._should_insert_newline(event):
            event.stop()
            event.prevent_default()
            self._insert_newline()
            return

        # ESC for autocomplete dismissal handled at the top of _on_key()

        if event.key == "up":
            if self._completions:
                event.stop()
                event.prevent_default()
                self._move_completion_selection(-1)
                return
            event.stop()
            event.prevent_default()
            if hasattr(self.app, "action_history_up"):
                self.app.action_history_up()
            return

        if event.key == "down":
            if self._completions:
                event.stop()
                event.prevent_default()
                self._move_completion_selection(1)
                return
            event.stop()
            event.prevent_default()
            if hasattr(self.app, "action_history_down"):
                self.app.action_history_down()
            return

        if event.key == "shift+tab":
            event.stop()
            event.prevent_default()
            if hasattr(self.app, "action_cycle_mode"):
                self.app.action_cycle_mode()
            return

        if event.key == "ctrl+shift+a":
            event.stop()
            event.prevent_default()
            if hasattr(self.app, "action_cycle_autonomy"):
                self.app.action_cycle_autonomy()
            return

        if event.key == "tab":
            event.stop()
            event.prevent_default()
            if self._accept_completion_selection():
                return
            if self.suggestion:
                self.insert(self.suggestion)
                return

            await super()._on_key(event)
            return

        if event.key in {"enter", "return"} and "+" not in event.key:
            if self._completions and self._accept_completion_selection():
                event.stop()
                event.prevent_default()
                return
            event.stop()
            event.prevent_default()
            self.post_message(self.Submitted(self, self.text))
            return

        await super()._on_key(event)
        self.update_suggestion()  # Refresh autocomplete after text changes

        if approval_mode and hasattr(app, "_render_approval_prompt"):
            app._render_approval_prompt()

    def on_paste(self, event: Paste) -> None:
        """Handle paste events, collapsing large blocks into placeholders."""

        paste_text = event.text
        event.stop()
        event.prevent_default()  # Prevent default paste behavior to avoid double paste

        if len(paste_text) > self._paste_threshold:
            token = self._register_large_paste(paste_text)
            self._replace_via_keyboard(token, *self.selection)
            self.update_suggestion()
            return

        self._replace_via_keyboard(paste_text, *self.selection)
        self.update_suggestion()
        app = getattr(self, "app", None)
        if getattr(app, "_approval_active", False) and hasattr(app, "_render_approval_prompt"):
            app._render_approval_prompt()

    @staticmethod
    def _should_insert_newline(event: Key) -> bool:
        """Return True if the key event should produce a newline."""

        newline_keys = {
            "shift+enter",
            "ctrl+j",
            "ctrl+shift+enter",
            "newline",
        }

        if event.key in newline_keys:
            return True

        if any(alias in newline_keys for alias in event.aliases):
            return True

        return event.character == "\n"

    def _insert_newline(self) -> None:
        """Insert a newline at the current cursor position, preserving selection."""

        start, end = self.selection
        self._replace_via_keyboard("\n", start, end)
        self.update_suggestion()

    def resolve_large_pastes(self, text: str) -> str:
        """Expand placeholder tokens back to the original pasted content."""

        for token, content in self._large_paste_cache.items():
            text = text.replace(token, content)
        return text

    def clear_large_pastes(self) -> None:
        """Clear cached large paste payloads after submission."""

        self._large_paste_cache.clear()

    # Mouse event handlers to prevent mouse escape sequences from being
    # interpreted as keyboard input (causes random characters in input)

    def on_mouse_down(self, event: MouseDown) -> None:
        """Handle mouse down - consume to prevent escape sequence leakage."""
        pass  # Let parent handle, but consume the event

    def on_mouse_move(self, event: MouseMove) -> None:
        """Handle mouse move - consume to prevent escape sequence leakage."""
        pass  # Consume mouse move events

    def on_mouse_up(self, event: MouseUp) -> None:
        """Handle mouse up - consume to prevent escape sequence leakage."""
        pass  # Consume mouse up events

    def _register_large_paste(self, content: str) -> str:
        """Store large paste content and return the placeholder token."""

        self._paste_counter += 1
        token = f"[[PASTE-{self._paste_counter}:{len(content)} chars]]"
        self._large_paste_cache[token] = content
        return token

    def move_cursor_to_end(self) -> None:
        """Position the cursor at the end of the current text content."""

        if not self.text:
            self.cursor_location = (0, 0)
            return

        lines = self.text.split("\n")
        if self.text.endswith("\n"):
            row = len(lines)
            column = 0
        else:
            row = len(lines) - 1
            column = len(lines[-1])

        self.cursor_location = (row, column)
        self.update_suggestion()


__all__ = ["ChatTextArea"]
