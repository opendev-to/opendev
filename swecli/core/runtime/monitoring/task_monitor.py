"""Task monitoring for tracking time, tokens, and interruption state."""

from __future__ import annotations

import threading
import time
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from swecli.core.runtime.interrupt_token import InterruptToken


class TaskMonitor:
    """Monitor task execution with timer, token tracking, and interruption support."""

    def __init__(self, interrupt_token: InterruptToken | None = None):
        """Initialize task monitor.

        Args:
            interrupt_token: Optional shared cancellation token. When provided,
                interrupt requests are delegated to the token so that all
                components sharing the same token observe the cancellation.
        """
        self._lock = threading.Lock()
        self._start_time: Optional[float] = None
        self._end_time: Optional[float] = None
        self._task_description: str = ""

        # Token tracking
        self._initial_tokens: int = 0
        self._current_tokens: int = 0
        self._token_delta: int = 0

        # Interruption
        self._interrupt_token: InterruptToken | None = interrupt_token
        self._interrupt_requested: bool = False
        self._is_running: bool = False

    def start(self, task_description: str, initial_tokens: int = 0) -> None:
        """Start monitoring a task.

        Args:
            task_description: Description of the task being monitored
            initial_tokens: Initial token count before task starts
        """
        with self._lock:
            self._start_time = time.time()
            self._end_time = None
            self._task_description = task_description
            self._initial_tokens = initial_tokens
            self._current_tokens = initial_tokens
            self._token_delta = 0
            self._interrupt_requested = False
            self._is_running = True

    def stop(self) -> dict:
        """Stop monitoring and return stats.

        Returns:
            Dictionary with task statistics:
            - elapsed_seconds: Total elapsed time
            - token_delta: Change in tokens (positive = increase, negative = decrease)
            - interrupted: Whether task was interrupted
            - task_description: Original task description
        """
        with self._lock:
            self._end_time = time.time()
            self._is_running = False

            # Calculate elapsed time directly to avoid nested lock
            if self._start_time is None:
                elapsed = 0
            else:
                elapsed = int(self._end_time - self._start_time)

            # Calculate token arrow directly to avoid nested lock
            if self._token_delta < 0:
                arrow = "↓"
            elif self._token_delta > 0:
                arrow = "↑"
            else:
                arrow = "·"

            return {
                "elapsed_seconds": elapsed,
                "token_delta": self._token_delta,
                "token_arrow": arrow,
                "interrupted": self._interrupt_requested,
                "task_description": self._task_description,
                "initial_tokens": self._initial_tokens,
                "current_tokens": self._current_tokens,
            }

    def update_tokens(self, current_tokens: int) -> None:
        """Update current token count.

        Args:
            current_tokens: Current total token count
        """
        with self._lock:
            self._current_tokens = current_tokens
            self._token_delta = current_tokens - self._initial_tokens

    def request_interrupt(self) -> None:
        """Request interruption of current task (called by ESC key handler)."""
        with self._lock:
            if self._interrupt_token is not None:
                self._interrupt_token.request()
            else:
                self._interrupt_requested = True

    def should_interrupt(self) -> bool:
        """Check if interruption has been requested.

        Returns:
            True if interruption requested, False otherwise
        """
        with self._lock:
            if self._interrupt_token is not None:
                return self._interrupt_token.is_requested()
            return self._interrupt_requested

    def set_interrupt_token(self, token: InterruptToken) -> None:
        """Attach a shared interrupt token after construction.

        Args:
            token: The InterruptToken to delegate interrupt state to.
        """
        with self._lock:
            self._interrupt_token = token

    def is_running(self) -> bool:
        """Check if task is currently running.

        Returns:
            True if running, False otherwise
        """
        with self._lock:
            return self._is_running

    def get_elapsed_seconds(self) -> int:
        """Get elapsed seconds since task started.

        Returns:
            Number of elapsed seconds (rounded to integer)
        """
        with self._lock:
            if self._start_time is None:
                return 0

            end = self._end_time if self._end_time else time.time()
            return int(end - self._start_time)

    def get_token_delta(self) -> int:
        """Get token delta (change since start).

        Returns:
            Token delta (positive = increase, negative = decrease)
        """
        with self._lock:
            return self._token_delta

    def _get_token_arrow(self) -> str:
        """Get arrow direction for token change.

        Returns:
            "↓" for decrease, "↑" for increase, "·" for no change
        """
        if self._token_delta < 0:
            return "↓"
        elif self._token_delta > 0:
            return "↑"
        return "·"

    def get_token_arrow(self) -> str:
        """Get arrow direction for token change (thread-safe public method).

        Returns:
            "↓" for decrease, "↑" for increase, "·" for no change
        """
        with self._lock:
            return self._get_token_arrow()

    def get_task_description(self) -> str:
        """Get current task description.

        Returns:
            Task description string
        """
        with self._lock:
            return self._task_description

    @staticmethod
    def format_tokens(count: int) -> str:
        """Format token count for display (e.g., 3700 -> 3.7k).

        Args:
            count: Token count to format

        Returns:
            Formatted token string
        """
        abs_count = abs(count)
        if abs_count >= 1000:
            formatted = f"{abs_count / 1000:.1f}k"
        else:
            formatted = str(abs_count)

        return formatted

    def get_formatted_token_display(self) -> str:
        """Get formatted token display with arrow.

        Returns:
            Formatted string like "↑ 3.7k tokens" or "↓ 1.2k tokens"
        """
        with self._lock:
            arrow = self._get_token_arrow()
            formatted = self.format_tokens(self._token_delta)

            if arrow == "·" or self._token_delta == 0:
                return ""  # Don't show if no change

            return f"{arrow} {formatted} tokens"
