"""Background task management service.

Manages long-running background processes with:
- File-based output storage (/tmp/opendev/<path>/tasks/<id>.output)
- PTY-based output streaming
- Status tracking and listeners for UI updates
"""

from __future__ import annotations

import os
import select
import signal
import threading
import time
import uuid
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum, auto
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional


class TaskStatus(Enum):
    """Status of a background task."""
    RUNNING = auto()
    COMPLETED = auto()
    FAILED = auto()
    KILLED = auto()


@dataclass
class BackgroundTask:
    """Represents a background task."""
    task_id: str
    command: str
    working_dir: Path
    pid: int
    status: TaskStatus
    started_at: datetime
    output_file: Path
    process: Any = None  # subprocess.Popen
    pty_master_fd: Optional[int] = None
    completed_at: Optional[datetime] = None
    exit_code: Optional[int] = None
    error_message: Optional[str] = None

    @property
    def runtime_seconds(self) -> float:
        """Get runtime in seconds."""
        end = self.completed_at or datetime.now()
        return (end - self.started_at).total_seconds()

    @property
    def is_running(self) -> bool:
        """Check if task is still running."""
        return self.status == TaskStatus.RUNNING


class BackgroundTaskManager:
    """Manages background tasks with file-based output storage."""

    def __init__(self, working_dir: Path):
        """Initialize task manager.

        Args:
            working_dir: Working directory for path hashing
        """
        self.working_dir = working_dir
        self._tasks: Dict[str, BackgroundTask] = {}
        self._lock = threading.RLock()
        self._output_threads: Dict[str, threading.Thread] = {}
        self._listeners: List[Callable[[str, TaskStatus], None]] = []
        self._stop_events: Dict[str, threading.Event] = {}

        # Create output directory
        self._output_dir = self._get_output_dir()
        self._output_dir.mkdir(parents=True, exist_ok=True)

    def _get_output_dir(self) -> Path:
        """Get output directory for background task output files.

        Returns:
            Path like /tmp/opendev/-Users-nghibui-codes-test-opencli/tasks/
        """
        # Convert path to safe directory name (replace / with -)
        cwd_str = str(self.working_dir.resolve())
        safe_path = cwd_str.replace("/", "-")
        return Path(f"/tmp/opendev/{safe_path}/tasks")

    def register_task(
        self,
        command: str,
        pid: int,
        process: Any,
        pty_master_fd: Optional[int] = None,
        initial_output: str = "",
    ) -> BackgroundTask:
        """Register a new background task.

        Args:
            command: The command being executed
            pid: Process ID
            process: subprocess.Popen object
            pty_master_fd: PTY master file descriptor for output streaming
            initial_output: Any output already captured during startup

        Returns:
            BackgroundTask object
        """
        task_id = uuid.uuid4().hex[:7]  # 7-char hex ID like Claude Code
        output_file = self._output_dir / f"{task_id}.output"

        task = BackgroundTask(
            task_id=task_id,
            command=command,
            working_dir=self.working_dir,
            pid=pid,
            status=TaskStatus.RUNNING,
            started_at=datetime.now(),
            output_file=output_file,
            process=process,
            pty_master_fd=pty_master_fd,
        )

        with self._lock:
            self._tasks[task_id] = task

        # Write initial output to file
        if initial_output:
            with open(output_file, "w") as f:
                f.write(initial_output)

        # Start output streaming thread if we have a PTY
        if pty_master_fd is not None:
            self._start_output_streaming(task)

        # Notify listeners
        self._notify_listeners(task_id, TaskStatus.RUNNING)

        return task

    def _start_output_streaming(self, task: BackgroundTask) -> None:
        """Start background thread to stream output from PTY to file.

        Args:
            task: The background task
        """
        stop_event = threading.Event()

        def stream_output():
            """Stream output from PTY to file."""
            if task.pty_master_fd is None:
                return

            try:
                with open(task.output_file, "a") as f:
                    while not stop_event.is_set():
                        # Check if there's data ready to read (non-blocking)
                        try:
                            ready, _, _ = select.select([task.pty_master_fd], [], [], 0.5)
                        except (ValueError, OSError):
                            # FD closed or invalid
                            break

                        if ready:
                            try:
                                data = os.read(task.pty_master_fd, 4096)
                                if data:
                                    text = data.decode("utf-8", errors="replace")
                                    f.write(text)
                                    f.flush()
                                else:
                                    # EOF - process likely died
                                    break
                            except OSError:
                                # Read error - process likely died
                                break

                        # Check if process is still running
                        if task.process and task.process.poll() is not None:
                            # Process exited, read any remaining output
                            try:
                                while True:
                                    ready, _, _ = select.select([task.pty_master_fd], [], [], 0.1)
                                    if not ready:
                                        break
                                    data = os.read(task.pty_master_fd, 4096)
                                    if not data:
                                        break
                                    f.write(data.decode("utf-8", errors="replace"))
                                    f.flush()
                            except (OSError, ValueError):
                                pass
                            break

            finally:
                # Update task status when streaming ends
                self._update_task_status(task)

        thread = threading.Thread(target=stream_output, daemon=True)
        thread.start()

        with self._lock:
            self._stop_events[task.task_id] = stop_event
            self._output_threads[task.task_id] = thread

    def _update_task_status(self, task: BackgroundTask) -> None:
        """Update task status based on process state.

        Args:
            task: The background task
        """
        if task.process is None:
            return

        exit_code = task.process.poll()
        if exit_code is None:
            return  # Still running

        with self._lock:
            task.completed_at = datetime.now()
            task.exit_code = exit_code

            if exit_code == 0:
                task.status = TaskStatus.COMPLETED
            elif exit_code == -signal.SIGTERM or exit_code == -signal.SIGKILL:
                task.status = TaskStatus.KILLED
            else:
                task.status = TaskStatus.FAILED
                task.error_message = f"Exited with code {exit_code}"

        self._notify_listeners(task.task_id, task.status)

    def get_running_tasks(self) -> List[BackgroundTask]:
        """Get all running tasks.

        Returns:
            List of tasks with RUNNING status
        """
        with self._lock:
            # First update status of all tasks
            for task in self._tasks.values():
                if task.status == TaskStatus.RUNNING:
                    self._update_task_status(task)
            return [t for t in self._tasks.values() if t.is_running]

    def get_all_tasks(self) -> List[BackgroundTask]:
        """Get all tasks (running and completed).

        Returns:
            List of all tasks sorted by start time (newest first)
        """
        with self._lock:
            # Update status of running tasks
            for task in self._tasks.values():
                if task.status == TaskStatus.RUNNING:
                    self._update_task_status(task)
            return sorted(
                self._tasks.values(),
                key=lambda t: t.started_at,
                reverse=True
            )

    def get_task(self, task_id: str) -> Optional[BackgroundTask]:
        """Get task by ID.

        Args:
            task_id: The task ID

        Returns:
            BackgroundTask or None if not found
        """
        with self._lock:
            task = self._tasks.get(task_id)
            if task and task.status == TaskStatus.RUNNING:
                self._update_task_status(task)
            return task

    def kill_task(self, task_id: str, sig: int = signal.SIGTERM) -> bool:
        """Kill a running task.

        Args:
            task_id: The task ID
            sig: Signal to send (default SIGTERM)

        Returns:
            True if killed successfully
        """
        with self._lock:
            task = self._tasks.get(task_id)
            if not task or not task.process:
                return False

            if task.process.poll() is not None:
                # Already dead
                return True

        try:
            # Stop output streaming
            with self._lock:
                stop_event = self._stop_events.get(task_id)
            if stop_event:
                stop_event.set()

            # Send signal
            task.process.send_signal(sig)

            # Wait up to 5 seconds for graceful shutdown
            try:
                task.process.wait(timeout=5)
            except Exception:
                # Force kill if still running
                task.process.kill()
                task.process.wait(timeout=2)

            # Update status
            with self._lock:
                task.completed_at = datetime.now()
                task.exit_code = task.process.poll()
                task.status = TaskStatus.KILLED

            self._notify_listeners(task_id, TaskStatus.KILLED)
            return True

        except Exception:
            return False

    def read_output(self, task_id: str, tail_lines: int = 100) -> str:
        """Read output from task's output file.

        Args:
            task_id: The task ID
            tail_lines: Number of lines to return (0 = all)

        Returns:
            Output string or empty string if not found
        """
        task = self.get_task(task_id)
        if not task or not task.output_file.exists():
            return ""

        try:
            with open(task.output_file, "r") as f:
                content = f.read()

            if tail_lines > 0:
                lines = content.splitlines()
                if len(lines) > tail_lines:
                    return "\n".join(lines[-tail_lines:])
                return content
            return content

        except Exception:
            return ""

    def add_listener(self, callback: Callable[[str, TaskStatus], None]) -> None:
        """Add status change listener.

        Args:
            callback: Function called with (task_id, status) on changes
        """
        self._listeners.append(callback)

    def remove_listener(self, callback: Callable[[str, TaskStatus], None]) -> None:
        """Remove status change listener.

        Args:
            callback: The callback to remove
        """
        if callback in self._listeners:
            self._listeners.remove(callback)

    def _notify_listeners(self, task_id: str, status: TaskStatus) -> None:
        """Notify all listeners of status change.

        Args:
            task_id: The task ID
            status: New status
        """
        for listener in self._listeners:
            try:
                listener(task_id, status)
            except Exception:
                pass  # Don't let listener errors break the manager

    def cleanup(self) -> None:
        """Clean up resources - kill all running tasks."""
        with self._lock:
            task_ids = list(self._tasks.keys())
        for task_id in task_ids:
            task = self.get_task(task_id)
            if task and task.is_running:
                self.kill_task(task_id)
            # Stop streaming threads
            with self._lock:
                stop_event = self._stop_events.get(task_id)
            if stop_event:
                stop_event.set()
