"""Background task status provider for UI integration.

Bridges the BackgroundTaskManager with the UI footer to show
running task count and handle status updates.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from textual.app import App
    from swecli.core.context_engineering.tools.background_task_manager import (
        BackgroundTaskManager,
        TaskStatus,
    )


class BackgroundTaskStatusProvider:
    """Provides task status updates to the UI footer.

    This class listens to task manager events and updates the
    footer's background task count indicator.
    """

    def __init__(
        self,
        app: "App",
        task_manager: "BackgroundTaskManager",
    ):
        """Initialize status provider.

        Args:
            app: The Textual app instance
            task_manager: The background task manager to monitor
        """
        self.app = app
        self.task_manager = task_manager

        # Register as listener for task status changes
        task_manager.add_listener(self._on_task_status_change)

        # Initial update
        self._update_footer()

    def _on_task_status_change(self, task_id: str, status: "TaskStatus") -> None:
        """Handle task status changes.

        Args:
            task_id: The task that changed
            status: New task status
        """
        self._update_footer()

    def _update_footer(self) -> None:
        """Update footer with current running task count."""
        running_count = len(self.task_manager.get_running_tasks())

        def _do_update():
            """Perform the actual footer update on UI thread."""
            # Get footer from app
            footer = getattr(self.app, 'footer', None)
            if footer is None:
                # Try to query for it
                try:
                    from swecli.ui_textual.widgets.status_bar import ModelFooter
                    footer = self.app.query_one(ModelFooter)
                except Exception:
                    return

            if footer and hasattr(footer, 'set_background_task_count'):
                footer.set_background_task_count(running_count)

        # Thread-safe UI update
        try:
            self.app.call_from_thread(_do_update)
        except RuntimeError:
            # Not in worker thread, call directly
            _do_update()

    def cleanup(self) -> None:
        """Clean up resources."""
        self.task_manager.remove_listener(self._on_task_status_change)
