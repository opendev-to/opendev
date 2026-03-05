"""Scroll controller for ConversationLog."""

from __future__ import annotations

import logging
from typing import Any

from textual.events import MouseDown, MouseMove, MouseScrollDown, MouseScrollUp, MouseUp
from textual.geometry import Size

from swecli.ui_textual.widgets.conversation.protocols import RichLogInterface

class DefaultScrollController:
    """Handles scrolling, auto-scroll logic, and input events for ConversationLog."""

    def __init__(self, log_widget: RichLogInterface, app: Any | None):
        self.log = log_widget
        self.app = app
        self._auto_scroll = True
        self._user_scrolled = False
        
        # Mouse drag detection for selection tip or just tracking state
        self._mouse_down_pos: tuple[int, int] | None = None

    @property
    def auto_scroll(self) -> bool:
        return self._auto_scroll
    
    @auto_scroll.setter
    def auto_scroll(self, value: bool) -> None:
        self._auto_scroll = value
        # Sync to underlying widget if it matters, but RichLog handles its own auto_scroll too
        # We might need to keep them in sync or just manage it here.
        # RichLog has an auto_scroll property.
        if hasattr(self.log, 'auto_scroll'):
            self.log.auto_scroll = value

    def on_key(self, event: Any) -> None:
        """Handle key events, delegating to overlays if active, else scrolling."""
        
        # Check for active overlays (Approval Prompt or Model Picker)
        if self.app:
            # 1. Approval Prompt
            approval_controller = getattr(self.app, "_approval_controller", None)
            if approval_controller and getattr(approval_controller, "active", False):
                if event.key == "up":
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_approval_move"):
                        self.app._approval_move(-1)
                    return
                if event.key == "down":
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_approval_move"):
                        self.app._approval_move(1)
                    return
                if event.key in {"enter", "return"} and "+" not in event.key:
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_approval_confirm"):
                        self.app._approval_confirm()
                    return
                if event.key in {"escape", "ctrl+c"}:
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_approval_cancel"):
                        self.app._approval_cancel()
                    return
                return  # Swallow other keys in approval mode

            # 2. Plan Approval
            plan_approval_controller = getattr(self.app, "_plan_approval_controller", None)
            if plan_approval_controller and getattr(plan_approval_controller, "active", False):
                if event.key == "up":
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_plan_approval_move"):
                        self.app._plan_approval_move(-1)
                    return
                if event.key == "down":
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_plan_approval_move"):
                        self.app._plan_approval_move(1)
                    return
                if event.key in {"enter", "return"} and "+" not in event.key:
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_plan_approval_confirm"):
                        self.app._plan_approval_confirm()
                    return
                if event.key in {"escape", "ctrl+c"}:
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_plan_approval_cancel"):
                        self.app._plan_approval_cancel()
                    return
                return  # Swallow other keys in plan approval mode

            # 3. Model Picker
            model_picker = getattr(self.app, "_model_picker", None)
            if model_picker and getattr(model_picker, "active", False):
                if event.key == "up":
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_model_picker_move"):
                        self.app._model_picker_move(-1)
                    return
                if event.key == "down":
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_model_picker_move"):
                        self.app._model_picker_move(1)
                    return
                if event.key in {"enter", "return"} and "+" not in event.key:
                    event.stop()
                    event.prevent_default()
                    confirm = getattr(self.app, "_model_picker_confirm", None)
                    if confirm is not None:
                        result = confirm()
                        import inspect
                        if inspect.isawaitable(result):
                            # We can't await here easily in sync method, 
                            # but usually key handlers in Textual are async. 
                            # If this method is called from async on_key in widget, we should likely be async too.
                            # BUT protocols.py defined it as sync... let's check.
                            pass 
                    return
                if event.key in {"escape", "ctrl+c"}:
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_model_picker_cancel"):
                        self.app._model_picker_cancel()
                    return
                if event.character and event.character.lower() == "b":
                    event.stop()
                    event.prevent_default()
                    if hasattr(self.app, "_model_picker_back"):
                        self.app._model_picker_back()
                    return

        # Default scrolling behavior
        # Handle Page Up/Down (or Fn+Up/Down) with a smaller stride for finer control
        if event.key == "pageup":
            self.scroll_partial_page(direction=-1)
            event.prevent_default()
            return

        elif event.key == "pagedown":
            self.scroll_partial_page(direction=1)
            event.prevent_default()
            return

        # For other scroll keys (arrows, home, end), mark as user-scrolled
        # The default behavior will handle the actual scrolling
        elif event.key in ("up", "down", "home", "end"):
            self._user_scrolled = True
            self.auto_scroll = False

    def scroll_partial_page(self, direction: int) -> None:
        """Scroll a fraction of the viewport instead of a full page."""
        self._user_scrolled = True
        self.auto_scroll = False
        
        # log.size might not be available if protocol doesn't have it, 
        # but RichLogInterface usually inherits from Widget so it has size.
        # We need to access size.height.
        height = getattr(self.log, 'size', Size(0, 20)).height
        stride = max(height // 10, 3)  # 10% of viewport per page
        
        if hasattr(self.log, 'scroll_relative'):
            self.log.scroll_relative(y=direction * stride)

    def on_mouse_scroll_down(self, event: MouseScrollDown) -> None:
        """Handle mouse scroll down (wheel down / two-finger swipe down)."""
        # When Option (meta) key is pressed, allow default behavior for text selection scrolling
        if event.meta:
            return  # Don't stop event, let terminal handle it
        self._user_scrolled = True
        self.auto_scroll = False
        if hasattr(self.log, 'scroll_relative'):
            self.log.scroll_relative(y=3)  # Scroll 3 lines per tick
        event.stop()

    def on_mouse_scroll_up(self, event: MouseScrollUp) -> None:
        """Handle mouse scroll up (wheel up / two-finger swipe up)."""
        if event.meta:
            return
        self._user_scrolled = True
        self.auto_scroll = False
        if hasattr(self.log, 'scroll_relative'):
            self.log.scroll_relative(y=-3)
        event.stop()

    def on_mouse_down(self, event: MouseDown) -> None:
        """Record position for potential click/drag detection."""
        self._mouse_down_pos = (event.x, event.y)

    def on_mouse_move(self, event: MouseMove) -> None:
        """Handle mouse move (dragging selection)."""
        if self._mouse_down_pos and event.button == 1:
            # User is dragging (selecting text), pause auto-scroll
            self._user_scrolled = True
            self.auto_scroll = False

    def on_mouse_up(self, event: MouseUp) -> None:
        """Handle mouse up (end of click/drag)."""
        self._mouse_down_pos = None

    def scroll_to_end(self, animate: bool = True) -> None:
        """Scroll to the bottom if auto-scroll is active."""
        if self._auto_scroll and not self._user_scrolled:
            if hasattr(self.log, 'scroll_end'):
                self.log.scroll_end(animate=animate)
        
        # If forced (animate=False usually implies forced update), reset user scroll?
        # Not necessarily. 
        # But if we want to force scroll to end, we should set auto_scroll=True.
        # Let's keep existing logic: only scroll if auto_scroll is True.

    def cleanup(self) -> None:
        pass
