"""Simple background scheduler for periodic tasks."""

from __future__ import annotations

import asyncio
import logging
from typing import Callable, Awaitable

logger = logging.getLogger(__name__)


class PeriodicTask:
    """A task that runs periodically."""

    def __init__(
        self,
        name: str,
        callback: Callable[[], Awaitable[None] | None],
        interval_seconds: float,
    ):
        self.name = name
        self.callback = callback
        self.interval = interval_seconds
        self._task: asyncio.Task | None = None

    async def _run(self) -> None:
        while True:
            try:
                await asyncio.sleep(self.interval)
                result = self.callback()
                if asyncio.iscoroutine(result):
                    await result
            except asyncio.CancelledError:
                break
            except Exception:
                logger.warning("Periodic task %s failed", self.name, exc_info=True)

    def start(self) -> None:
        if self._task is None or self._task.done():
            self._task = asyncio.create_task(self._run())

    def stop(self) -> None:
        if self._task and not self._task.done():
            self._task.cancel()


class BackgroundScheduler:
    """Manages periodic background tasks."""

    def __init__(self) -> None:
        self._tasks: dict[str, PeriodicTask] = {}

    def add_task(
        self,
        name: str,
        callback: Callable[[], Awaitable[None] | None],
        interval_seconds: float,
    ) -> None:
        """Register a periodic task."""
        if name in self._tasks:
            self._tasks[name].stop()
        self._tasks[name] = PeriodicTask(name, callback, interval_seconds)

    def start_all(self) -> None:
        """Start all registered tasks."""
        for task in self._tasks.values():
            task.start()
        if self._tasks:
            logger.info("Started %d background tasks", len(self._tasks))

    def stop_all(self) -> None:
        """Stop all running tasks."""
        for task in self._tasks.values():
            task.stop()

    def start_task(self, name: str) -> None:
        """Start a specific task by name."""
        if name in self._tasks:
            self._tasks[name].start()

    def stop_task(self, name: str) -> None:
        """Stop a specific task by name."""
        if name in self._tasks:
            self._tasks[name].stop()
