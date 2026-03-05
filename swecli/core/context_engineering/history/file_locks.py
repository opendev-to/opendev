"""Cross-process file locking for concurrent session access.

Uses fcntl.flock() on macOS/Linux for exclusive locks on session files.
This prevents corruption when multiple channel handlers (Telegram, WhatsApp, etc.)
write to the same session simultaneously.
"""

import fcntl
import logging
import time
from contextlib import contextmanager
from pathlib import Path

logger = logging.getLogger(__name__)


@contextmanager
def exclusive_session_lock(session_file: Path, timeout: float = 10.0):
    """Acquire an exclusive lock on a session file for safe concurrent writes.

    Uses fcntl.flock() which works across processes on macOS and Linux.
    The lock is automatically released when the context manager exits.

    Example:
        with exclusive_session_lock(session_file):
            # Safe to write to session file
            with open(session_file, 'w') as f:
                json.dump(data, f)

    Args:
        session_file: Path to the session file to lock
        timeout: Maximum time in seconds to wait for lock acquisition (default: 10.0)

    Raises:
        TimeoutError: If lock cannot be acquired within timeout period

    Note:
        On Windows, fcntl is not available. The lock becomes a no-op (silent fallback).
        For production Windows support, use msvcrt.locking() or filelock library.
    """
    # Create lock file in same directory as session file
    lock_file = session_file.with_suffix(session_file.suffix + ".lock")

    try:
        fd = lock_file.open("w")
    except OSError as e:
        logger.warning(f"Could not create lock file {lock_file}: {e}")
        # Fallback: no locking (single-process mode)
        yield
        return

    try:
        # Try to acquire exclusive lock with timeout
        start = time.monotonic()
        while True:
            try:
                # LOCK_EX: exclusive lock, LOCK_NB: non-blocking
                fcntl.flock(fd.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                logger.debug(f"Acquired lock on {session_file.name}")
                break
            except (BlockingIOError, OSError):
                # Lock held by another process
                elapsed = time.monotonic() - start
                if elapsed > timeout:
                    raise TimeoutError(
                        f"Could not acquire lock on {session_file} after {timeout}s"
                    )
                # Brief sleep before retry
                time.sleep(0.05)

        # Lock acquired - yield control to caller
        yield

    finally:
        # Release lock and clean up
        try:
            fcntl.flock(fd.fileno(), fcntl.LOCK_UN)
            logger.debug(f"Released lock on {session_file.name}")
        except Exception as e:
            logger.warning(f"Error releasing lock on {session_file.name}: {e}")

        try:
            fd.close()
        except Exception:
            pass

        try:
            lock_file.unlink(missing_ok=True)
        except Exception as e:
            logger.debug(f"Could not remove lock file {lock_file}: {e}")
