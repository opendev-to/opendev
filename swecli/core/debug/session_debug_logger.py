"""Per-session structured debug logger.

Writes JSONL events to ~/.opendev/sessions/{session_id}.debug when --verbose is enabled.
Each line is a JSON object: {"ts": "...", "elapsed_ms": ..., "event": "...", "component": "...", "data": {...}}
"""

import json
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional


# Maximum length for string values in event data (prevents bloated logs)
_MAX_PREVIEW_LEN = 200


def _truncate(value: Any, max_len: int = _MAX_PREVIEW_LEN) -> Any:
    """Truncate string values to prevent huge log entries."""
    if isinstance(value, str) and len(value) > max_len:
        return value[:max_len] + f"... ({len(value)} chars)"
    return value


class SessionDebugLogger:
    """Per-session structured debug logger.

    Writes JSONL events to ~/.opendev/sessions/{session_id}.debug.
    Thread-safe via threading.Lock() for background thread writes.

    When verbose is disabled, use the noop() classmethod to get a logger
    whose log() method returns immediately with zero overhead.
    """

    def __init__(self, session_dir: Path, session_id: str):
        self._file = Path(session_dir) / f"{session_id}.debug"
        self._lock = threading.Lock()
        self._start_time = time.monotonic()
        self._enabled = True
        # Ensure parent directory exists
        self._file.parent.mkdir(parents=True, exist_ok=True)

    def log(self, event: str, component: str, **data: Any) -> None:
        """Write a debug event as a JSONL line.

        Args:
            event: Event type (e.g. "llm_call_start", "tool_call_end")
            component: Component name (e.g. "react", "tool", "llm")
            **data: Arbitrary event data (values are truncated if too long)
        """
        if not self._enabled:
            return

        elapsed_ms = int((time.monotonic() - self._start_time) * 1000)
        ts = datetime.now(timezone.utc).isoformat()

        # Truncate string values in data
        truncated_data = {k: _truncate(v) for k, v in data.items()}

        entry = {
            "ts": ts,
            "elapsed_ms": elapsed_ms,
            "event": event,
            "component": component,
            "data": truncated_data,
        }

        line = json.dumps(entry, default=str) + "\n"

        with self._lock:
            with open(self._file, "a", encoding="utf-8") as f:
                f.write(line)

    @property
    def file_path(self) -> Path:
        """Return the path to the debug log file (None for noop loggers)."""
        return self._file

    @classmethod
    def noop(cls) -> "SessionDebugLogger":
        """Return a no-op logger (zero overhead when verbose is off)."""
        instance = cls.__new__(cls)
        instance._enabled = False
        instance._file = None
        instance._lock = None
        instance._start_time = 0
        return instance


# Module-level singleton
_current_logger: Optional[SessionDebugLogger] = None


def get_debug_logger() -> SessionDebugLogger:
    """Get the current session debug logger (or a no-op instance)."""
    global _current_logger
    if _current_logger is None:
        return SessionDebugLogger.noop()
    return _current_logger


def set_debug_logger(logger: Optional[SessionDebugLogger]) -> None:
    """Set the current session debug logger."""
    global _current_logger
    _current_logger = logger
