"""Todo panel widget for displaying persistent todo list."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

from textual.widgets import Static

from swecli.ui_textual.style_tokens import SUCCESS, WARNING

logger = logging.getLogger(__name__)

if TYPE_CHECKING:
    from swecli.ui_textual.managers.spinner_service import SpinnerService, SpinnerFrame


class TodoPanel(Static):
    """Persistent todo panel showing all todos with status indicators.

    Displays the complete todo list with:
    - Active todos (yellow ▶)
    - Pending todos (gray ○)
    - Completed todos (gray ✓ with strikethrough)

    Toggle visibility with Ctrl+T.
    """

    def __init__(self, todo_handler=None, **kwargs):
        """Initialize the todo panel.

        Args:
            todo_handler: TodoHandler instance for accessing todo state
            **kwargs: Additional widget kwargs
        """
        super().__init__(**kwargs)
        self.todo_handler = todo_handler
        self.border_title = "TODOS"
        self.is_expanded = False  # Track collapsed/expanded state

        # SpinnerService integration
        self._spinner_service: Optional["SpinnerService"] = None
        self._spinner_id: str = ""
        self._render_suppressed: bool = False  # Prevents spinner race conditions

    def set_spinner_service(self, service: "SpinnerService") -> None:
        """Inject the SpinnerService.

        Args:
            service: The centralized SpinnerService instance
        """
        self._spinner_service = service

    def on_mount(self) -> None:
        """Called when widget is mounted to the DOM."""
        self.refresh_display()

    def refresh_display(self) -> None:
        """Update the panel with current todos, respecting collapsed/expanded state.

        Shows collapsed summary or full list depending on is_expanded state.
        Auto-shows panel in collapsed state when todos are created.
        """
        # CRITICAL: Suppress spinner rendering to prevent race conditions
        # Any queued spinner frame callbacks will see this flag and bail out
        self._render_suppressed = True

        if not self.todo_handler:
            self.update("Todo panel not connected")
            return

        todos = list(self.todo_handler._todos.values())

        logger.debug(f"[PANEL] refresh_display: {len(todos)} todos")
        for t in todos:
            logger.debug(f"[PANEL]   {t.id}: status={t.status}, title={t.title[:30]}...")

        # Hide when no todos at all
        if not todos:
            self._stop_spinner()
            self.update("")
            self.border_title = ""
            if self.has_class("collapsed"):
                self.remove_class("collapsed")
            if self.has_class("expanded"):
                self.remove_class("expanded")
            return

        # Auto-hide panel when all todos are complete
        if all(t.status == "done" for t in todos):
            logger.debug("[PANEL] All done, auto-hiding panel")
            self._stop_spinner()
            self.update("")
            self.border_title = ""
            self.remove_class("collapsed")
            self.remove_class("expanded")
            return

        # Auto-show in collapsed state when todos are created
        if not self.has_class("collapsed") and not self.has_class("expanded"):
            self.add_class("collapsed")
            self.is_expanded = False

        # Render based on current state
        if self.is_expanded:
            self._render_expanded(todos)
        else:
            self._render_collapsed(todos)

    def _render_collapsed(self, todos: list) -> None:
        """Render compact summary with animated spinner for active tasks."""
        active_text = self._get_active_todo_text(todos)

        if active_text:
            # Un-suppress - we want the spinner to animate
            self._render_suppressed = False

            # Start spinner if not already running
            if not self._spinner_id:
                self._start_spinner(active_text)
            else:
                # Update the active text in case it changed
                if self._spinner_service:
                    self._spinner_service.update_metadata(self._spinner_id, active_text=active_text)
            # Initial render will happen via callback
        else:
            # Keep suppressed - we're showing completion count, not spinner
            self._stop_spinner()

            # Now safe to show completion progress
            completed = len([t for t in todos if t.status == "done"])
            total = len(todos)
            summary = f"{completed}/{total} completed [dim](Press Ctrl+T to expand/hide)[/dim]"
            self.update(summary)

        self.border_title = ""  # No border title in collapsed mode

    def _render_expanded(self, todos: list) -> None:
        """Render full todo list with status indicators."""
        # Update border title with count
        self.border_title = f"TODOS ({len(todos)} total)"

        # Sort todos by status: doing -> todo -> done
        status_order = {"doing": 0, "todo": 1, "done": 2}
        sorted_todos = sorted(todos, key=lambda t: (status_order.get(t.status, 3), t.id))

        # Build display content
        lines = []

        for todo in sorted_todos:
            if todo.status == "done":
                # Completed: gray with strikethrough
                lines.append(f"[dim]✓ [strike]{todo.title}[/strike][/dim]")
            elif todo.status == "doing":
                # In-progress: yellow
                lines.append(f"[{WARNING}]▶ {todo.title}[/{WARNING}]")
            else:
                # Pending: gray
                lines.append(f"[dim]○ {todo.title}[/dim]")

        # Join all lines and update display
        self.update("\n".join(lines))

    def _get_active_todo_text(self, todos: list) -> str | None:
        """Get the activeForm or title of the currently active todo.

        Args:
            todos: List of TodoItem objects

        Returns:
            The active_form (or title as fallback) of the first todo with status "doing",
            or None if no active todo
        """
        for todo in todos:
            if todo.status == "doing":
                # Prefer active_form (present continuous) for spinner display
                return todo.active_form if todo.active_form else todo.title
        return None

    def _start_spinner(self, active_text: str) -> None:
        """Start the spinner animation via SpinnerService.

        Args:
            active_text: The text to display with the spinner
        """
        if self._spinner_service is None:
            return

        # Stop any existing spinner
        self._stop_spinner()

        # Import here to avoid circular import
        from swecli.ui_textual.managers.spinner_service import SpinnerType

        # Register with SpinnerService
        self._spinner_id = self._spinner_service.register(
            spinner_type=SpinnerType.TODO,
            render_callback=self._on_spinner_frame,
            metadata={"active_text": active_text},
        )

    def _stop_spinner(self) -> None:
        """Stop the spinner via SpinnerService."""
        if self._spinner_id and self._spinner_service:
            self._spinner_service.stop(self._spinner_id)
        self._spinner_id = ""

    def _on_spinner_frame(self, frame: "SpinnerFrame") -> None:
        """Callback invoked by SpinnerService for each animation frame.

        Args:
            frame: SpinnerFrame containing animation data
        """
        # Guard 0: Don't render if suppressed (prevents race conditions during refresh)
        if self._render_suppressed:
            return

        # Guard 1: Don't render if expanded mode
        if self.is_expanded:
            return

        # Guard 2: Don't render if spinner was stopped (ID cleared)
        if not self._spinner_id:
            return

        # Guard 3: Don't render if no active todo exists
        if self.todo_handler:
            todos = list(self.todo_handler._todos.values())
            if not any(t.status == "doing" for t in todos):
                return

        active_text = frame.metadata.get("active_text", "")
        summary = f"[{WARNING}]{frame.char} {active_text}[/{WARNING}] [dim](Press Ctrl+T to expand/hide)[/dim]"
        self.update(summary)

    def toggle_expansion(self) -> None:
        """Toggle between collapsed and expanded states."""
        if self.is_expanded:
            # Collapse
            self.is_expanded = False
            self.remove_class("expanded")
            self.add_class("collapsed")
        else:
            # Expand
            self.is_expanded = True
            self.remove_class("collapsed")
            self.add_class("expanded")

            # Stop spinner when expanding
            self._stop_spinner()

        self.refresh_display()

    def on_unmount(self) -> None:
        """Clean up spinner when widget unmounts."""
        self._stop_spinner()
