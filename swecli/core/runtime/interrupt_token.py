"""Centralized interrupt/cancellation token for a single agent run.

One token is created per user query execution. All components (LLM caller,
tool executor, HTTP client, etc.) share the same token so that a single
ESC press reliably cancels the entire operation regardless of which phase
is active.
"""

import threading


class InterruptToken:
    """Thread-safe cancellation token shared across all components of a run.

    Usage:
        token = InterruptToken()
        # Pass to all execution components
        # UI calls token.request() on ESC
        # Components poll token.is_requested() or call token.throw_if_requested()
    """

    def __init__(self) -> None:
        self._event = threading.Event()

    def request(self) -> None:
        """Signal that the user wants to cancel the current operation."""
        self._event.set()

    def is_requested(self) -> bool:
        """Check whether cancellation has been requested.

        Returns:
            True if request() has been called.
        """
        return self._event.is_set()

    def throw_if_requested(self) -> None:
        """Raise InterruptedError if cancellation was requested.

        Raises:
            InterruptedError: When the token has been triggered.
        """
        if self._event.is_set():
            raise InterruptedError("Interrupted by user")

    def reset(self) -> None:
        """Clear the cancellation signal (use with care)."""
        self._event.clear()

    # Duck-typing compatibility with TaskMonitor interface so existing
    # code that calls ``monitor.should_interrupt()`` works unchanged.
    def should_interrupt(self) -> bool:
        """Alias for is_requested() — TaskMonitor compatibility."""
        return self.is_requested()

    def request_interrupt(self) -> None:
        """Alias for request() — TaskMonitor compatibility."""
        self.request()
